import XCTest
import YevuneCoreFFI
@testable import Yevune

@MainActor
final class AccessControlViewModelTests: XCTestCase {
    func testAccessRuleSelectionDropsHiddenPrincipalsAndSortsCompleteGrantList() {
        let grants = [
            Principal(principalType: .role, id: "ro-hidden-admin"),
            Principal(principalType: .user, id: "us-listener-b"),
            Principal(principalType: .user, id: "us-hidden-admin"),
            Principal(principalType: .role, id: "ro-listeners"),
            Principal(principalType: .user, id: "us-listener-a"),
        ]
        let selection = AccessRuleSelection(
            grants: grants,
            assignableUserIDs: ["us-listener-a", "us-listener-b"],
            assignableRoleIDs: ["ro-listeners"]
        )

        XCTAssertEqual(selection.userIDs, ["us-listener-a", "us-listener-b"])
        XCTAssertEqual(selection.roleIDs, ["ro-listeners"])
        XCTAssertEqual(
            selection.principals,
            [
                Principal(principalType: .user, id: "us-listener-a"),
                Principal(principalType: .user, id: "us-listener-b"),
                Principal(principalType: .role, id: "ro-listeners"),
            ]
        )
        XCTAssertFalse(selection.isEmpty)
    }

    func testAccessRuleSelectionTreatsHiddenOnlyGrantsAsEmpty() {
        let selection = AccessRuleSelection(
            grants: [
                Principal(principalType: .user, id: "us-hidden-admin"),
                Principal(principalType: .role, id: "ro-hidden-admin"),
            ],
            assignableUserIDs: ["us-listener"],
            assignableRoleIDs: ["ro-listeners"]
        )

        XCTAssertTrue(selection.userIDs.isEmpty)
        XCTAssertTrue(selection.roleIDs.isEmpty)
        XCTAssertTrue(selection.principals.isEmpty)
        XCTAssertTrue(selection.isEmpty)
    }

    func testRuleEditorIdentityIgnoresGrantOrderButChangesWithEditableContent() {
        let original = AccessRule(
            id: "ru-1",
            scopeType: .album,
            scopeId: "al-1",
            scopeName: "Blue Train",
            grants: [
                Principal(principalType: .role, id: "ro-2"),
                Principal(principalType: .user, id: "us-2"),
            ]
        )
        let reordered = AccessRule(
            id: "ru-1",
            scopeType: .album,
            scopeId: "al-1",
            scopeName: "Renamed display only",
            grants: Array(original.grants.reversed())
        )
        let changedGrants = AccessRule(
            id: "ru-1",
            scopeType: .album,
            scopeId: "al-1",
            scopeName: "Blue Train",
            grants: [Principal(principalType: .user, id: "us-3")]
        )
        let changedScope = AccessRule(
            id: "ru-1",
            scopeType: .artist,
            scopeId: "ar-1",
            scopeName: "John Coltrane",
            grants: original.grants
        )

        XCTAssertEqual(AccessRuleEditorIdentity(rule: original), AccessRuleEditorIdentity(rule: reordered))
        XCTAssertNotEqual(AccessRuleEditorIdentity(rule: original), AccessRuleEditorIdentity(rule: changedGrants))
        XCTAssertNotEqual(AccessRuleEditorIdentity(rule: original), AccessRuleEditorIdentity(rule: changedScope))
    }

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

    func testFailedTargetSearchClearsResultsFromPreviousSuccessfulSearch() async {
        let fake = FakeAccessControlClient(searchResult: Self.searchResult)
        let model = AccessControlViewModel(client: fake)
        await model.searchTargets(scopeType: .album, query: "blue")
        XCTAssertFalse(model.targetResults.isEmpty)
        await fake.setSearchFails(true)

        await model.searchTargets(scopeType: .album, query: "missing")

        XCTAssertTrue(model.targetResults.isEmpty)
        XCTAssertNotNil(model.errorMessage)
        XCTAssertFalse(model.isSearching)
    }

    func testSaveAndRestoreForwardExactDTOsAndReloadRules() async {
        let fake = FakeAccessControlClient(rules: Self.rules, users: Self.users, roles: Self.roles)
        let model = AccessControlViewModel(client: fake)
        await model.load()
        let target = AccessScopeTarget(
            scopeType: .genre,
            id: "摇滚 & Blues",
            name: "摇滚 & Blues",
            context: nil
        )
        let grants = [
            Principal(principalType: .user, id: "us-2"),
            Principal(principalType: .role, id: "ro-7"),
        ]

        let saved = await model.saveRule(target: target, grants: grants)
        XCTAssertTrue(saved)
        var calls = await fake.recordedCalls()
        XCTAssertTrue(calls.contains(.set(.genre, "摇滚 & Blues", grants)))
        XCTAssertEqual(model.rule(for: target)?.grants, grants)

        let restored = await model.restoreFamilyVisibility(ruleID: "ru-1")
        XCTAssertTrue(restored)
        calls = await fake.recordedCalls()
        XCTAssertTrue(calls.contains(.delete("ru-1")))
        XCTAssertFalse(model.rules.contains(where: { $0.id == "ru-1" }))
    }

    func testFailedWriteReturnsFalseAndPreservesPreviousStateWithoutReloading() async {
        let fake = FakeAccessControlClient(
            rules: Self.rules,
            users: Self.users,
            roles: Self.roles,
            mutationFails: true
        )
        let model = AccessControlViewModel(client: fake)
        await model.load()
        let target = AccessScopeTarget(scopeType: .genre, id: "Jazz", name: "Jazz", context: nil)

        let succeeded = await model.saveRule(target: target, grants: [])

        XCTAssertFalse(succeeded)
        XCTAssertEqual(model.rules, Self.rules)
        XCTAssertEqual(model.users, Self.users)
        XCTAssertEqual(model.roles, Self.roles)
        XCTAssertNotNil(model.errorMessage)
        XCTAssertFalse(model.isMutating)
        let calls = await fake.recordedCalls()
        XCTAssertEqual(calls.filter { $0 == .listRules }.count, 1)
        XCTAssertEqual(calls.filter { $0 == .listUsers }.count, 1)
        XCTAssertEqual(calls.filter { $0 == .listRoles }.count, 1)
    }

    func testSuccessfulWriteWithFailedReloadReturnsTrueAndPreservesPreviousState() async {
        let fake = FakeAccessControlClient(
            rules: Self.rules,
            users: Self.users,
            roles: Self.roles,
            failListRulesOnCall: 2
        )
        let model = AccessControlViewModel(client: fake)
        await model.load()
        let target = AccessScopeTarget(scopeType: .genre, id: "Jazz", name: "Jazz", context: nil)

        let succeeded = await model.saveRule(target: target, grants: [])

        XCTAssertTrue(succeeded)
        XCTAssertEqual(model.rules, Self.rules)
        XCTAssertEqual(model.users, Self.users)
        XCTAssertEqual(model.roles, Self.roles)
        XCTAssertTrue(model.errorMessage?.contains("操作已完成") == true)
        XCTAssertFalse(model.isMutating)
        XCTAssertFalse(model.isLoading)
    }

    func testRuleLookupMatchesBothScopeTypeAndOpaqueID() async {
        let model = AccessControlViewModel(
            client: FakeAccessControlClient(rules: Self.rules, users: Self.users, roles: Self.roles)
        )
        await model.load()

        XCTAssertEqual(
            model.rule(
                for: AccessScopeTarget(scopeType: .track, id: "tr-1", name: "ignored", context: nil)
            )?.id,
            "ru-1"
        )
        XCTAssertNil(
            model.rule(
                for: AccessScopeTarget(scopeType: .album, id: "tr-1", name: "ignored", context: nil)
            )
        )
    }

    func testReferenceCountsRespectPrincipalTypeAndEmptyGrantNeedsConfirmation() async {
        let rules = Self.rules + [
            AccessRule(
                id: "ru-3",
                scopeType: .artist,
                scopeId: "ar-1",
                scopeName: "John Coltrane",
                grants: [
                    Principal(principalType: .user, id: "us-2"),
                    Principal(principalType: .role, id: "us-2"),
                    Principal(principalType: .user, id: "ro-2"),
                ]
            ),
        ]
        let model = AccessControlViewModel(
            client: FakeAccessControlClient(rules: rules, users: Self.users, roles: Self.roles)
        )
        await model.load()

        XCTAssertEqual(model.ruleReferenceCount(userID: "us-2"), 2)
        XCTAssertEqual(model.ruleReferenceCount(roleID: "ro-2"), 1)
        XCTAssertTrue(model.requiresEmptyGrantConfirmation([]))
        XCTAssertFalse(
            model.requiresEmptyGrantConfirmation([Principal(principalType: .user, id: "us-2")])
        )
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

private enum AccessControlCall: Equatable {
    case listRules
    case listUsers
    case listRoles
    case set(ScopeType, String, [Principal])
    case delete(String)
}

private actor FakeAccessControlClient: MusicClientProviding {
    private var rules: [AccessRule]
    private let users: [User]
    private let roles: [Role]
    private let genres: [Genre]
    private let searchResult: SearchResult
    private var searchFails: Bool
    private var loadFailure: LoadFailure?
    private let mutationFails: Bool
    private let failListRulesOnCall: Int?
    private var queries: [String] = []
    private var calls: [AccessControlCall] = []
    private var listRulesCallCount = 0

    init(
        rules: [AccessRule] = [],
        users: [User] = [],
        roles: [Role] = [],
        genres: [Genre] = [],
        searchResult: SearchResult = .init(artists: [], albums: [], tracks: []),
        loadFailure: LoadFailure? = nil,
        searchFails: Bool = false,
        mutationFails: Bool = false,
        failListRulesOnCall: Int? = nil
    ) {
        self.rules = rules
        self.users = users
        self.roles = roles
        self.genres = genres
        self.searchResult = searchResult
        self.loadFailure = loadFailure
        self.searchFails = searchFails
        self.mutationFails = mutationFails
        self.failListRulesOnCall = failListRulesOnCall
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
        calls.append(.listRules)
        listRulesCallCount += 1
        if loadFailure == .rules || listRulesCallCount == failListRulesOnCall {
            throw TestError.requestFailed
        }
        return rules
    }

    func listUsers() async throws -> [User] {
        calls.append(.listUsers)
        if loadFailure == .users { throw TestError.requestFailed }
        return users
    }

    func listRoles() async throws -> [Role] {
        calls.append(.listRoles)
        if loadFailure == .roles { throw TestError.requestFailed }
        return roles
    }

    func setAccessRule(
        scopeType: ScopeType,
        scopeID: String,
        grants: [Principal]
    ) async throws -> AccessRule {
        calls.append(.set(scopeType, scopeID, grants))
        if mutationFails { throw TestError.requestFailed }
        let rule = AccessRule(
            id: rules.first(where: { $0.scopeType == scopeType && $0.scopeId == scopeID })?.id ?? "ru-new",
            scopeType: scopeType,
            scopeId: scopeID,
            scopeName: scopeID,
            grants: grants
        )
        rules.removeAll { $0.scopeType == scopeType && $0.scopeId == scopeID }
        rules.append(rule)
        return rule
    }

    func deleteAccessRule(id: String) async throws {
        calls.append(.delete(id))
        if mutationFails { throw TestError.requestFailed }
        rules.removeAll { $0.id == id }
    }

    func setLoadFailure(_ failure: LoadFailure?) {
        loadFailure = failure
    }

    func setSearchFails(_ fails: Bool) {
        searchFails = fails
    }

    func searchQueries() -> [String] { queries }
    func recordedCalls() -> [AccessControlCall] { calls }
}

private enum TestError: Error {
    case requestFailed
}
