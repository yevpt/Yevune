import XCTest
import YevuneCoreFFI
@testable import Yevune

@MainActor
final class AdminViewModelTests: XCTestCase {
    func testLoadPublishesUsersAndRolesAndPreservesValidSelection() async {
        let client = FakeAdminClient(users: Self.users, roles: Self.roles)
        let model = AdminViewModel(currentUsername: "owner", client: client)
        model.selectedUserID = "us-2"
        model.selectedRoleID = "ro-2"

        await model.load()

        XCTAssertEqual(model.users, Self.users)
        XCTAssertEqual(model.roles, Self.roles)
        XCTAssertEqual(model.selectedUserID, "us-2")
        XCTAssertEqual(model.selectedRoleID, "ro-2")
        XCTAssertNil(model.errorMessage)
        XCTAssertFalse(model.isLoading)
    }

    func testFilteredUsersMatchesNameOrEmailIgnoringCase() async {
        let model = AdminViewModel(
            currentUsername: "owner",
            client: FakeAdminClient(users: Self.users, roles: Self.roles)
        )
        await model.load()

        model.query = "LISTENER"
        XCTAssertEqual(model.filteredUsers.map(\.id), ["us-2"])

        model.query = "FAMILY.EXAMPLE"
        XCTAssertEqual(model.filteredUsers.map(\.id), ["us-2"])
    }

    func testCurrentAndLastAdministratorCannotBeDeletedOrDemoted() async {
        let model = AdminViewModel(
            currentUsername: "owner",
            client: FakeAdminClient(users: Self.users, roles: Self.roles)
        )
        await model.load()

        XCTAssertFalse(model.canDelete(Self.users[0]))
        XCTAssertFalse(model.canSetAdmin(Self.users[0], to: false))
        XCTAssertTrue(model.canDelete(Self.users[1]))
    }

    func testBuiltInRoleCannotBeDeletedAndCustomRoleReportsAffectedUsers() async {
        let model = AdminViewModel(
            currentUsername: "owner",
            client: FakeAdminClient(users: Self.users, roles: Self.roles)
        )
        await model.load()

        XCTAssertFalse(model.canDelete(Self.roles[0]))
        XCTAssertTrue(model.canDelete(Self.roles[1]))
        XCTAssertEqual(model.affectedUserCount(for: Self.roles[1]), 1)
    }

    func testUserMutationsForwardExactValuesAndReloadState() async {
        let client = FakeAdminClient(users: Self.users, roles: Self.roles)
        let model = AdminViewModel(currentUsername: "owner", client: client)
        await model.load()

        await model.createUser(
            name: "小 明",
            email: "new+family@example.com",
            password: "p&a ss",
            admin: false
        )
        await model.updateUser(Self.users[1], email: "listener+new@example.com", admin: true)
        await model.changePassword(for: Self.users[1], password: "新 密码&1")
        await model.deleteUser(Self.users[1])

        let calls = await client.recordedCalls()
        XCTAssertTrue(calls.contains(.createUser("小 明", "new+family@example.com", "p&a ss", false)))
        XCTAssertTrue(calls.contains(.updateUser("Listener", "listener+new@example.com", true)))
        XCTAssertTrue(calls.contains(.changePassword("Listener", "新 密码&1")))
        XCTAssertTrue(calls.contains(.deleteUser("Listener")))
        XCTAssertEqual(calls.filter { $0 == .listUsers }.count, 5)
        XCTAssertEqual(calls.filter { $0 == .listRoles }.count, 5)
        XCTAssertFalse(model.users.contains(where: { $0.id == "us-2" }))
    }

    func testRoleMutationsForwardOpaqueIDsAndReloadState() async {
        let client = FakeAdminClient(users: Self.users, roles: Self.roles)
        let model = AdminViewModel(currentUsername: "owner", client: client)
        await model.load()

        await model.createRole(name: "家 人 & friends")
        await model.setRole(Self.roles[1], assigned: true, for: Self.users[1])
        await model.setRole(Self.roles[1], assigned: false, for: Self.users[1])
        await model.deleteRole(Self.roles[1])

        let calls = await client.recordedCalls()
        XCTAssertTrue(calls.contains(.createRole("家 人 & friends")))
        XCTAssertTrue(calls.contains(.assignRole("us-2", "ro-2")))
        XCTAssertTrue(calls.contains(.unassignRole("us-2", "ro-2")))
        XCTAssertTrue(calls.contains(.deleteRole("ro-2")))
        XCTAssertEqual(calls.filter { $0 == .listUsers }.count, 5)
        XCTAssertEqual(calls.filter { $0 == .listRoles }.count, 5)
        XCTAssertFalse(model.roles.contains(where: { $0.id == "ro-2" }))
    }

    func testFailedMutationPublishesErrorWithoutReloadingOrDiscardingState() async {
        let client = FakeAdminClient(users: Self.users, roles: Self.roles, failMutations: true)
        let model = AdminViewModel(currentUsername: "owner", client: client)
        await model.load()

        await model.createRole(name: "family")

        let calls = await client.recordedCalls()
        XCTAssertNotNil(model.errorMessage)
        XCTAssertFalse(model.isMutating)
        XCTAssertEqual(model.users, Self.users)
        XCTAssertEqual(model.roles, Self.roles)
        XCTAssertEqual(calls.filter { $0 == .listUsers }.count, 1)
        XCTAssertEqual(calls.filter { $0 == .listRoles }.count, 1)
    }

    private static let users = [
        User(
            id: "us-1",
            name: "owner",
            email: "owner@example.com",
            created: "2026-07-14T00:00:00Z",
            admin: true,
            roles: ["admin"]
        ),
        User(
            id: "us-2",
            name: "Listener",
            email: "listener@family.example",
            created: nil,
            admin: false,
            roles: ["listeners"]
        ),
    ]

    private static let roles = [
        Role(id: "ro-1", name: "admin", isBuiltin: true),
        Role(id: "ro-2", name: "listeners", isBuiltin: false),
    ]
}

private enum AdminCall: Equatable {
    case listUsers
    case listRoles
    case createUser(String, String, String, Bool)
    case updateUser(String, String, Bool)
    case changePassword(String, String)
    case deleteUser(String)
    case createRole(String)
    case deleteRole(String)
    case assignRole(String, String)
    case unassignRole(String, String)
}

private actor FakeAdminClient: MusicClientProviding {
    private var users: [User]
    private var roles: [Role]
    private var calls: [AdminCall] = []
    private let failMutations: Bool

    init(users: [User] = [], roles: [Role] = [], failMutations: Bool = false) {
        self.users = users
        self.roles = roles
        self.failMutations = failMutations
    }

    func login(server: String, user: String, password: String) async throws -> SessionValue {
        SessionValue(server: server, user: user, admin: true)
    }

    func listAlbums(offset: UInt32, size: UInt32) async throws -> [Album] { [] }
    func search(query: String) async throws -> SearchResult { SearchResult(artists: [], albums: [], tracks: []) }
    func upload(localPath: String, libraryKey: String, progress: UploadProgress) async throws -> Track {
        throw CocoaError(.featureUnsupported)
    }
    func updateTags(id: String, update: TagUpdate) async throws {}
    func deleteTrack(id: String) async throws {}
    func moveTrack(id: String, key: String) async throws {}
    func startScan() async throws -> ScanStatus { ScanStatus(scanning: false, count: 0) }
    func scanStatus() async throws -> ScanStatus { ScanStatus(scanning: false, count: 0) }
    func listUsers() async throws -> [User] {
        calls.append(.listUsers)
        return users
    }

    func listRoles() async throws -> [Role] {
        calls.append(.listRoles)
        return roles
    }

    func createUser(username: String, email: String, password: String, admin: Bool) async throws {
        calls.append(.createUser(username, email, password, admin))
        try failIfRequested()
        users.append(User(id: "us-new", name: username, email: email, created: nil, admin: admin, roles: []))
    }

    func updateUser(username: String, email: String, admin: Bool) async throws {
        calls.append(.updateUser(username, email, admin))
        try failIfRequested()
        guard let index = users.firstIndex(where: { $0.name == username }) else { return }
        users[index].email = email
        users[index].admin = admin
    }

    func changePassword(username: String, password: String) async throws {
        calls.append(.changePassword(username, password))
        try failIfRequested()
    }

    func deleteUser(username: String) async throws {
        calls.append(.deleteUser(username))
        try failIfRequested()
        users.removeAll { $0.name == username }
    }

    func createRole(name: String) async throws -> Role {
        calls.append(.createRole(name))
        try failIfRequested()
        let role = Role(id: "ro-new", name: name, isBuiltin: false)
        roles.append(role)
        return role
    }

    func deleteRole(id: String) async throws {
        calls.append(.deleteRole(id))
        try failIfRequested()
        roles.removeAll { $0.id == id }
    }

    func assignRole(userID: String, roleID: String) async throws {
        calls.append(.assignRole(userID, roleID))
        try failIfRequested()
        guard let userIndex = users.firstIndex(where: { $0.id == userID }),
              let role = roles.first(where: { $0.id == roleID }),
              !users[userIndex].roles.contains(role.name) else { return }
        users[userIndex].roles.append(role.name)
    }

    func unassignRole(userID: String, roleID: String) async throws {
        calls.append(.unassignRole(userID, roleID))
        try failIfRequested()
        guard let userIndex = users.firstIndex(where: { $0.id == userID }),
              let role = roles.first(where: { $0.id == roleID }) else { return }
        users[userIndex].roles.removeAll { $0 == role.name }
    }

    func recordedCalls() -> [AdminCall] { calls }

    private func failIfRequested() throws {
        if failMutations {
            throw CocoaError(.fileWriteNoPermission)
        }
    }
}
