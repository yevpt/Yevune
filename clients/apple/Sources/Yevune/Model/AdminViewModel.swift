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

    func createUser(name: String, email: String, password: String, admin: Bool) async {
        await mutate {
            try await client.createUser(
                username: name,
                email: email,
                password: password,
                admin: admin
            )
        }
    }

    func updateUser(_ user: User, email: String, admin: Bool) async {
        guard canSetAdmin(user, to: admin) else { return }
        await mutate {
            try await client.updateUser(username: user.name, email: email, admin: admin)
        }
    }

    func changePassword(for user: User, password: String) async {
        await mutate {
            try await client.changePassword(username: user.name, password: password)
        }
    }

    func deleteUser(_ user: User) async {
        guard canDelete(user) else { return }
        await mutate {
            try await client.deleteUser(username: user.name)
        }
    }

    func createRole(name: String) async {
        await mutate {
            _ = try await client.createRole(name: name)
        }
    }

    func deleteRole(_ role: Role) async {
        guard canDelete(role) else { return }
        await mutate {
            try await client.deleteRole(id: role.id)
        }
    }

    func setRole(_ role: Role, assigned: Bool, for user: User) async {
        await mutate {
            if assigned {
                try await client.assignRole(userID: user.id, roleID: role.id)
            } else {
                try await client.unassignRole(userID: user.id, roleID: role.id)
            }
        }
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

    private func mutate(_ operation: () async throws -> Void) async {
        guard !isMutating else { return }
        isMutating = true
        errorMessage = nil
        defer { isMutating = false }

        do {
            try await operation()
            await load()
        } catch {
            errorMessage = error.localizedDescription
        }
    }
}
