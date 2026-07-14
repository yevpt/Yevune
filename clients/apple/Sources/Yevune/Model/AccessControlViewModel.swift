import Foundation
import YevuneCoreFFI

struct AccessScopeTarget: Identifiable, Hashable {
    let scopeType: ScopeType
    let id: String
    let name: String
    let context: String?
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
    @Published private(set) var errorMessage: String?

    private let client: any MusicClientProviding

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

    func load() async {
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
            errorMessage = error.localizedDescription
        }
    }

    func searchTargets(scopeType: ScopeType, query: String) async {
        let needle = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !needle.isEmpty else {
            targetResults = []
            return
        }

        isSearching = true
        errorMessage = nil
        defer { isSearching = false }

        do {
            if scopeType == .genre {
                targetResults = try await client.listGenres()
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
                    targetResults = result.tracks.map {
                        AccessScopeTarget(
                            scopeType: .track,
                            id: $0.id,
                            name: $0.title,
                            context: $0.album
                        )
                    }
                case .album:
                    targetResults = result.albums.map {
                        AccessScopeTarget(
                            scopeType: .album,
                            id: $0.id,
                            name: $0.name,
                            context: $0.artist
                        )
                    }
                case .artist:
                    targetResults = result.artists.map {
                        AccessScopeTarget(
                            scopeType: .artist,
                            id: $0.id,
                            name: $0.name,
                            context: "\($0.albumCount) 张专辑"
                        )
                    }
                case .genre:
                    targetResults = []
                }
            }
        } catch {
            errorMessage = error.localizedDescription
        }
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
        if let selectedRuleID, !rules.contains(where: { $0.id == selectedRuleID }) {
            self.selectedRuleID = nil
        }
    }
}
