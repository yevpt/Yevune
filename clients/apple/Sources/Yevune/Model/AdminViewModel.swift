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
            apply(try await fetchState())
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
        if !newValue && user.name == currentUsername { return false }
        return newValue || !user.admin || administratorCount > 1
    }

    func canDelete(_ role: Role) -> Bool {
        !role.isBuiltin
    }

    func affectedUserCount(for role: Role) -> Int {
        users.count { $0.roles.contains(role.name) }
    }

    @discardableResult
    func createUser(name: String, email: String, password: String, admin: Bool) async -> Bool {
        await mutate {
            try await client.createUser(
                username: name,
                email: email,
                password: password,
                admin: admin
            )
        }
    }

    @discardableResult
    func updateUser(_ user: User, email: String, admin: Bool) async -> Bool {
        guard canSetAdmin(user, to: admin) else { return false }
        return await mutate {
            try await client.updateUser(username: user.name, email: email, admin: admin)
        }
    }

    @discardableResult
    func changePassword(for user: User, password: String) async -> Bool {
        await mutate {
            try await client.changePassword(username: user.name, password: password)
        }
    }

    @discardableResult
    func deleteUser(_ user: User) async -> Bool {
        guard canDelete(user) else { return false }
        return await mutate {
            try await client.deleteUser(username: user.name)
        }
    }

    @discardableResult
    func createRole(name: String) async -> Bool {
        await mutate {
            _ = try await client.createRole(name: name)
        }
    }

    @discardableResult
    func deleteRole(_ role: Role) async -> Bool {
        guard canDelete(role) else { return false }
        return await mutate {
            try await client.deleteRole(id: role.id)
        }
    }

    @discardableResult
    func setRole(_ role: Role, assigned: Bool, for user: User) async -> Bool {
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

    private func fetchState() async throws -> ([User], [Role]) {
        async let fetchedUsers = client.listUsers()
        async let fetchedRoles = client.listRoles()
        return try await (fetchedUsers, fetchedRoles)
    }

    private func apply(_ state: ([User], [Role])) {
        users = state.0
        roles = state.1
        preserveValidSelections()
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
