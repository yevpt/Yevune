import Foundation
import YevuneCoreFFI

struct AccessScopeTarget: Identifiable, Hashable {
    let scopeType: ScopeType
    let id: String
    let name: String
    let context: String?

    static func fromAlbum(_ album: Album) -> AccessScopeTarget {
        AccessScopeTarget(
            scopeType: .album,
            id: album.id,
            name: album.name,
            context: album.artist
        )
    }

    static func artist(from album: Album) -> AccessScopeTarget? {
        guard let artistID = album.artistId, !artistID.isEmpty else { return nil }
        return AccessScopeTarget(
            scopeType: .artist,
            id: artistID,
            name: album.artist ?? artistID,
            context: album.name
        )
    }

    static func fromTrack(_ track: Track) -> AccessScopeTarget {
        AccessScopeTarget(
            scopeType: .track,
            id: track.id,
            name: track.title,
            context: track.album
        )
    }

    static func fromGenre(_ genre: String) -> AccessScopeTarget {
        AccessScopeTarget(
            scopeType: .genre,
            id: genre,
            name: genre,
            context: nil
        )
    }
}

enum AccessEditorPresentation: Equatable {
    case editor
    case loading
    case unavailable(String)
}

enum AccessManagementPolicy {
    static func allowsEntry(isAdmin: Bool) -> Bool {
        isAdmin
    }

    static func editorPresentation(
        hasLoadedSuccessfully: Bool,
        isLoading: Bool,
        errorMessage: String?
    ) -> AccessEditorPresentation {
        if isLoading { return .loading }
        if hasLoadedSuccessfully { return .editor }
        return .unavailable(errorMessage ?? "访问控制数据尚未加载。")
    }
}

@MainActor
final class AccessControlViewModel: ObservableObject {
    @Published private(set) var rules: [AccessRule] = []
    @Published private(set) var users: [User] = []
    @Published private(set) var roles: [Role] = []
    @Published private(set) var targetResults: [AccessScopeTarget] = []
    @Published var query = ""
    @Published var scopeFilter: ScopeType?
    @Published var selectedRuleID: String?
    @Published private(set) var isLoading = false
    @Published private(set) var isSearching = false
    @Published private(set) var isMutating = false
    @Published private(set) var hasLoadedSuccessfully = false
    @Published private(set) var errorMessage: String?
    @Published private(set) var searchErrorMessage: String?

    private let client: any MusicClientProviding
    private var searchGeneration: UInt64 = 0

    init(client: any MusicClientProviding) {
        self.client = client
    }

    var assignableUsers: [User] {
        users.filter { !$0.admin }
    }

    var assignableRoles: [Role] {
        roles.filter { $0.name != "admin" }
    }

    var filteredRules: [AccessRule] {
        let needle = query.trimmingCharacters(in: .whitespacesAndNewlines)
        return rules.filter { rule in
            let matchesScope = scopeFilter == nil || rule.scopeType == scopeFilter
            let matchesQuery = needle.isEmpty
                || rule.scopeId.localizedCaseInsensitiveContains(needle)
                || (rule.scopeName?.localizedCaseInsensitiveContains(needle) ?? false)
            return matchesScope && matchesQuery
        }
    }

    func rule(for target: AccessScopeTarget) -> AccessRule? {
        rules.first { $0.scopeType == target.scopeType && $0.scopeId == target.id }
    }

    func ruleReferenceCount(userID: String) -> Int {
        rules.count { rule in
            rule.grants.contains { $0.principalType == .user && $0.id == userID }
        }
    }

    func ruleReferenceCount(roleID: String) -> Int {
        rules.count { rule in
            rule.grants.contains { $0.principalType == .role && $0.id == roleID }
        }
    }

    func requiresEmptyGrantConfirmation(_ grants: [Principal]) -> Bool {
        grants.isEmpty
    }

    func load() async {
        guard !isLoading else { return }
        isLoading = true
        errorMessage = nil
        defer { isLoading = false }

        do {
            apply(try await fetchState())
        } catch {
            rules = []
            users = []
            roles = []
            selectedRuleID = nil
            hasLoadedSuccessfully = false
            errorMessage = error.localizedDescription
        }
    }

    func searchTargets(scopeType: ScopeType, query: String) async {
        searchGeneration &+= 1
        let generation = searchGeneration
        searchErrorMessage = nil

        let needle = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !needle.isEmpty else {
            targetResults = []
            isSearching = false
            return
        }

        isSearching = true

        do {
            let results: [AccessScopeTarget]
            if scopeType == .genre {
                results = try await client.listGenres()
                    .filter { $0.value.localizedCaseInsensitiveContains(needle) }
                    .map {
                        AccessScopeTarget(
                            scopeType: .genre,
                            id: $0.value,
                            name: $0.value,
                            context: "\($0.songCount) 首"
                        )
                    }
            } else {
                let result = try await client.search(query: needle)
                switch scopeType {
                case .track:
                    results = result.tracks.map {
                        AccessScopeTarget(
                            scopeType: .track,
                            id: $0.id,
                            name: $0.title,
                            context: $0.album
                        )
                    }
                case .album:
                    results = result.albums.map {
                        AccessScopeTarget(
                            scopeType: .album,
                            id: $0.id,
                            name: $0.name,
                            context: $0.artist
                        )
                    }
                case .artist:
                    results = result.artists.map {
                        AccessScopeTarget(
                            scopeType: .artist,
                            id: $0.id,
                            name: $0.name,
                            context: "\($0.albumCount) 张专辑"
                        )
                    }
                case .genre:
                    results = []
                }
            }
            guard generation == searchGeneration else { return }
            targetResults = results
            isSearching = false
        } catch {
            guard generation == searchGeneration else { return }
            targetResults = []
            searchErrorMessage = error.localizedDescription
            isSearching = false
        }
    }

    @discardableResult
    func saveRule(target: AccessScopeTarget, grants: [Principal]) async -> Bool {
        await mutate {
            _ = try await client.setAccessRule(
                scopeType: target.scopeType,
                scopeID: target.id,
                grants: grants
            )
        }
    }

    @discardableResult
    func restoreFamilyVisibility(ruleID: String) async -> Bool {
        await mutate {
            try await client.deleteAccessRule(id: ruleID)
        }
    }

    func refreshAfterPrincipalDeletion(succeeded: Bool) async {
        guard succeeded else { return }
        await load()
    }

    private func fetchState() async throws -> ([AccessRule], [User], [Role]) {
        async let fetchedRules = client.listAccessRules()
        async let fetchedUsers = client.listUsers()
        async let fetchedRoles = client.listRoles()
        return try await (fetchedRules, fetchedUsers, fetchedRoles)
    }

    private func apply(_ state: ([AccessRule], [User], [Role])) {
        rules = state.0
        users = state.1
        roles = state.2
        hasLoadedSuccessfully = true
        if let selectedRuleID, !rules.contains(where: { $0.id == selectedRuleID }) {
            self.selectedRuleID = nil
        }
    }

    private func mutate(_ operation: () async throws -> Void) async -> Bool {
        guard !isMutating else { return false }
        isMutating = true
        errorMessage = nil
        defer { isMutating = false }

        do {
            try await operation()
        } catch {
            errorMessage = error.localizedDescription
            return false
        }

        isLoading = true
        defer { isLoading = false }
        do {
            apply(try await fetchState())
        } catch {
            errorMessage = "操作已完成，但刷新失败：\(error.localizedDescription)"
        }
        return true
    }
}
