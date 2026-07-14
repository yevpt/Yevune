import Foundation
import YevuneCoreFFI

@MainActor
final class AdminViewModel: ObservableObject {
    @Published private(set) var users: [User] = []
    @Published private(set) var roles: [Role] = []
    @Published var query = ""
    @Published var selectedUserID: String?
    @Published var selectedRoleID: String?
    @Published private(set) var isLoading = false
    @Published private(set) var isMutating = false
    @Published private(set) var errorMessage: String?

    let currentUsername: String

    private let client: any MusicClientProviding

    init(currentUsername: String, client: any MusicClientProviding) {
        self.currentUsername = currentUsername
        self.client = client
    }

    var filteredUsers: [User] {
        let term = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !term.isEmpty else { return users }
        return users.filter { user in
            user.name.localizedCaseInsensitiveContains(term)
                || (user.email?.localizedCaseInsensitiveContains(term) ?? false)
        }
    }

    var customRoles: [Role] {
        roles.filter { !$0.isBuiltin }
    }

    func load() async {
        isLoading = true
        errorMessage = nil
        defer { isLoading = false }

        do {
            async let fetchedUsers = client.listUsers()
            async let fetchedRoles = client.listRoles()
            let (newUsers, newRoles) = try await (fetchedUsers, fetchedRoles)
            users = newUsers
            roles = newRoles
            preserveValidSelections()
        } catch {
            users = []
            roles = []
            selectedUserID = nil
            selectedRoleID = nil
            errorMessage = error.localizedDescription
        }
    }

    func canDelete(_ user: User) -> Bool {
        guard user.name != currentUsername else { return false }
        return !user.admin || administratorCount > 1
    }

    func canSetAdmin(_ user: User, to newValue: Bool) -> Bool {
        newValue || !user.admin || administratorCount > 1
    }

    func canDelete(_ role: Role) -> Bool {
        !role.isBuiltin
    }

    func affectedUserCount(for role: Role) -> Int {
        users.count { $0.roles.contains(role.name) }
    }

    private var administratorCount: Int {
        users.count { $0.admin }
    }

    private func preserveValidSelections() {
        if let selectedUserID, !users.contains(where: { $0.id == selectedUserID }) {
            self.selectedUserID = nil
        }
        if let selectedRoleID, !roles.contains(where: { $0.id == selectedRoleID }) {
            self.selectedRoleID = nil
        }
    }
}
