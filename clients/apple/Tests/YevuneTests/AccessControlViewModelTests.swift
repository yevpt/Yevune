import XCTest
import YevuneCoreFFI
@testable import Yevune

@MainActor
final class AccessControlViewModelTests: XCTestCase {
    func testLoadPublishesStatePreservesValidSelectionAndExcludesAdministrators() async {
        let fake = FakeAccessControlClient(
            rules: Self.rules,
            users: Self.users,
            roles: Self.roles
        )
        let model = AccessControlViewModel(client: fake)
        model.selectedRuleID = "ru-2"

        await model.load()

        XCTAssertEqual(model.rules, Self.rules)
        XCTAssertEqual(model.users, Self.users)
        XCTAssertEqual(model.roles, Self.roles)
        XCTAssertEqual(model.selectedRuleID, "ru-2")
        XCTAssertEqual(model.assignableUsers.map(\.name), ["listener"])
        XCTAssertFalse(model.assignableRoles.contains(where: { $0.name == "admin" }))
        XCTAssertNil(model.errorMessage)
        XCTAssertFalse(model.isLoading)
    }

    func testLoadClearsSelectionThatIsAbsentFromNewRules() async {
        let fake = FakeAccessControlClient(rules: Self.rules, users: Self.users, roles: Self.roles)
        let model = AccessControlViewModel(client: fake)
        model.selectedRuleID = "ru-missing"

        await model.load()

        XCTAssertNil(model.selectedRuleID)
    }

    func testAnyLoadFailureClearsAllEditableStateAndSelection() async {
        for failure in LoadFailure.allCases {
            let fake = FakeAccessControlClient(
                rules: Self.rules,
                users: Self.users,
                roles: Self.roles,
                loadFailure: nil
            )
            let model = AccessControlViewModel(client: fake)
            await model.load()
            model.selectedRuleID = "ru-2"
            await fake.setLoadFailure(failure)

            await model.load()

            XCTAssertTrue(model.rules.isEmpty, "failure: \(failure)")
            XCTAssertTrue(model.users.isEmpty, "failure: \(failure)")
            XCTAssertTrue(model.roles.isEmpty, "failure: \(failure)")
            XCTAssertNil(model.selectedRuleID, "failure: \(failure)")
            XCTAssertNotNil(model.errorMessage, "failure: \(failure)")
            XCTAssertFalse(model.isLoading, "failure: \(failure)")
        }
    }

    func testFilteredRulesMatchesNameOrIDIgnoringCaseAndAppliesScope() async {
        let model = AccessControlViewModel(
            client: FakeAccessControlClient(rules: Self.rules, users: Self.users, roles: Self.roles)
        )
        await model.load()

        model.query = "BLUE"
        XCTAssertEqual(model.filteredRules.map(\.id), ["ru-2"])

        model.query = "TR-1"
        XCTAssertEqual(model.filteredRules.map(\.id), ["ru-1"])

        model.query = ""
        model.scopeFilter = .album
        XCTAssertEqual(model.filteredRules.map(\.id), ["ru-2"])
    }

    func testSearchTargetsMapsTrackAlbumAndArtistResultsByRequestedScope() async {
        let fake = FakeAccessControlClient(searchResult: Self.searchResult)
        let model = AccessControlViewModel(client: fake)

        await model.searchTargets(scopeType: .track, query: " blue ")
        XCTAssertEqual(
            model.targetResults,
            [AccessScopeTarget(scopeType: .track, id: "tr-1", name: "Blue Train", context: "Blue Train")]
        )

        await model.searchTargets(scopeType: .album, query: "blue")
        XCTAssertEqual(
            model.targetResults,
            [AccessScopeTarget(scopeType: .album, id: "al-1", name: "Blue Train", context: "John Coltrane")]
        )

        await model.searchTargets(scopeType: .artist, query: "blue")
        XCTAssertEqual(
            model.targetResults,
            [AccessScopeTarget(scopeType: .artist, id: "ar-1", name: "John Coltrane", context: "3 张专辑")]
        )
        let queries = await fake.searchQueries()
        XCTAssertEqual(queries, ["blue", "blue", "blue"])
        XCTAssertFalse(model.isSearching)
    }

    func testGenreSearchFiltersIgnoringCaseWithoutCallingGeneralSearch() async {
        let fake = FakeAccessControlClient(
            genres: [
                Genre(value: "Rock", songCount: 12, albumCount: 2),
                Genre(value: "Jazz", songCount: 8, albumCount: 1),
            ],
            searchResult: Self.searchResult
        )
        let model = AccessControlViewModel(client: fake)

        await model.searchTargets(scopeType: .genre, query: "rock")

        XCTAssertEqual(
            model.targetResults,
            [AccessScopeTarget(scopeType: .genre, id: "Rock", name: "Rock", context: "12 首")]
        )
        let queries = await fake.searchQueries()
        XCTAssertEqual(queries, [])
    }

    func testEmptyTargetQueryClearsPreviousResultsWithoutSearching() async {
        let fake = FakeAccessControlClient(searchResult: Self.searchResult)
        let model = AccessControlViewModel(client: fake)
        await model.searchTargets(scopeType: .album, query: "blue")

        await model.searchTargets(scopeType: .album, query: " \n ")

        XCTAssertTrue(model.targetResults.isEmpty)
        let queries = await fake.searchQueries()
        XCTAssertEqual(queries, ["blue"])
    }

    func testFailedTargetSearchPublishesErrorAndStopsSearching() async {
        let fake = FakeAccessControlClient(searchFails: true)
        let model = AccessControlViewModel(client: fake)

        await model.searchTargets(scopeType: .album, query: "blue")

        XCTAssertNotNil(model.errorMessage)
        XCTAssertFalse(model.isSearching)
        XCTAssertTrue(model.targetResults.isEmpty)
    }

    private static let rules = [
        AccessRule(
            id: "ru-1",
            scopeType: .track,
            scopeId: "tr-1",
            scopeName: "Naima",
            grants: [Principal(principalType: .user, id: "us-2")]
        ),
        AccessRule(
            id: "ru-2",
            scopeType: .album,
            scopeId: "al-1",
            scopeName: "Blue Train",
            grants: [Principal(principalType: .role, id: "ro-2")]
        ),
    ]

    private static let users = [
        User(id: "us-1", name: "owner", email: nil, created: nil, admin: true, roles: ["admin"]),
        User(id: "us-2", name: "listener", email: nil, created: nil, admin: false, roles: ["listeners"]),
    ]

    private static let roles = [
        Role(id: "ro-1", name: "admin", isBuiltin: true),
        Role(id: "ro-2", name: "listeners", isBuiltin: false),
    ]

    private static let searchResult = SearchResult(
        artists: [
            Artist(
                id: "ar-1",
                name: "John Coltrane",
                sortName: nil,
                coverArt: nil,
                musicBrainzId: nil,
                albumCount: 3
            ),
        ],
        albums: [
            Album(
                id: "al-1",
                name: "Blue Train",
                artist: "John Coltrane",
                artistId: "ar-1",
                coverArt: nil,
                songCount: 5,
                duration: 2_400,
                year: 1957,
                genre: "Jazz",
                created: nil
            ),
        ],
        tracks: [
            Track(
                id: "tr-1",
                title: "Blue Train",
                album: "Blue Train",
                albumId: "al-1",
                artist: "John Coltrane",
                artistId: "ar-1",
                track: 1,
                discNumber: 1,
                year: 1957,
                genre: "Jazz",
                coverArt: nil,
                size: 1_024,
                contentType: "audio/flac",
                suffix: "flac",
                duration: 640,
                bitRate: 900,
                created: nil,
                path: nil
            ),
        ]
    )
}

private enum LoadFailure: CaseIterable {
    case rules
    case users
    case roles
}

private actor FakeAccessControlClient: MusicClientProviding {
    private let rules: [AccessRule]
    private let users: [User]
    private let roles: [Role]
    private let genres: [Genre]
    private let searchResult: SearchResult
    private let searchFails: Bool
    private var loadFailure: LoadFailure?
    private var queries: [String] = []

    init(
        rules: [AccessRule] = [],
        users: [User] = [],
        roles: [Role] = [],
        genres: [Genre] = [],
        searchResult: SearchResult = .init(artists: [], albums: [], tracks: []),
        loadFailure: LoadFailure? = nil,
        searchFails: Bool = false
    ) {
        self.rules = rules
        self.users = users
        self.roles = roles
        self.genres = genres
        self.searchResult = searchResult
        self.loadFailure = loadFailure
        self.searchFails = searchFails
    }

    func login(server: String, user: String, password: String) async throws -> SessionValue {
        SessionValue(server: server, user: user)
    }

    func listAlbums(offset: UInt32, size: UInt32) async throws -> [Album] { [] }

    func listGenres() async throws -> [Genre] {
        if loadFailure == .roles { throw TestError.requestFailed }
        return genres
    }

    func search(query: String) async throws -> SearchResult {
        queries.append(query)
        if searchFails { throw TestError.requestFailed }
        return searchResult
    }

    func upload(localPath: String, libraryKey: String, progress: UploadProgress) async throws -> Track {
        throw CocoaError(.featureUnsupported)
    }

    func updateTags(id: String, update: TagUpdate) async throws {}
    func deleteTrack(id: String) async throws {}
    func moveTrack(id: String, key: String) async throws {}
    func startScan() async throws -> ScanStatus { .init(scanning: false, count: 0) }
    func scanStatus() async throws -> ScanStatus { .init(scanning: false, count: 0) }

    func listAccessRules() async throws -> [AccessRule] {
        if loadFailure == .rules { throw TestError.requestFailed }
        return rules
    }

    func listUsers() async throws -> [User] {
        if loadFailure == .users { throw TestError.requestFailed }
        return users
    }

    func listRoles() async throws -> [Role] {
        if loadFailure == .roles { throw TestError.requestFailed }
        return roles
    }

    func setLoadFailure(_ failure: LoadFailure?) {
        loadFailure = failure
    }

    func searchQueries() -> [String] { queries }
}

private enum TestError: Error {
    case requestFailed
}
