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

private actor FakeAdminClient: MusicClientProviding {
    private let users: [User]
    private let roles: [Role]

    init(users: [User] = [], roles: [Role] = []) {
        self.users = users
        self.roles = roles
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
    func listUsers() async throws -> [User] { users }
    func listRoles() async throws -> [Role] { roles }
}
