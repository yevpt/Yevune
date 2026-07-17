import XCTest
import YevuneCoreFFI
@testable import Yevune

@MainActor
final class LoginViewModelTests: XCTestCase {
    func testLoginDefaultsToLocalServer() {
        let model = LoginViewModel(client: FakeMusicClient())
        XCTAssertEqual(model.server, "http://localhost:4533")
    }

    func testLaunchCoordinatorMakesApplicationVisibleAndActive() {
        let application = FakeApplicationActivation()
        LaunchCoordinator.activate(application)
        XCTAssertTrue(application.didSetRegularPolicy)
        XCTAssertTrue(application.didActivateIgnoringOtherApps)
    }
    func testProductionBridgeConformsToViewModelProtocol() {
        let client: any MusicClientProviding = CoreMusicClient()
        XCTAssertNotNil(client as AnyObject)
    }

    func testMusicClientProtocolExposesAccessRules() async throws {
        let rule = AccessRule(
            id: "rule-1",
            scopeType: .album,
            scopeId: "album-1",
            scopeName: "Album",
            grants: [Principal(principalType: .role, id: "role-1")]
        )
        let client: any MusicClientProviding = FakeMusicClient(accessRules: [rule])

        let rules = try await client.listAccessRules()

        XCTAssertEqual(rules, [rule])
    }

    func testMusicClientProtocolForwardsPagedSearchRequestAndResponse() async throws {
        let response = SearchPage(
            artists: [],
            albums: [],
            tracks: [],
            hasMoreArtists: false,
            hasMoreAlbums: true,
            hasMoreTracks: false
        )
        let spy = FakeMusicClient(searchPage: response)
        let client: any MusicClientProviding = spy
        let request = SearchPageRequest(
            query: "blue", artistOffset: 1, artistCount: 24,
            albumOffset: 2, albumCount: 24,
            trackOffset: 3, trackCount: 24
        )

        let page = try await client.searchPage(request: request)
        let recordedRequest = await spy.recordedSearchPageRequest()

        XCTAssertEqual(page.hasMoreArtists, false)
        XCTAssertEqual(page.hasMoreAlbums, true)
        XCTAssertEqual(page.hasMoreTracks, false)
        XCTAssertEqual(recordedRequest?.query, "blue")
        XCTAssertEqual(recordedRequest?.artistOffset, 1)
        XCTAssertEqual(recordedRequest?.artistCount, 24)
        XCTAssertEqual(recordedRequest?.albumOffset, 2)
        XCTAssertEqual(recordedRequest?.albumCount, 24)
        XCTAssertEqual(recordedRequest?.trackOffset, 3)
        XCTAssertEqual(recordedRequest?.trackCount, 24)
    }

    func testSubmitPublishesAuthenticatedSession() async {
        let client = FakeMusicClient()
        let model = LoginViewModel(client: client)
        model.server = "http://music.local:4533"
        model.user = "admin"
        model.password = "secret"

        await model.submit()

        XCTAssertEqual(
            model.session,
            .init(server: "http://music.local:4533", user: "admin", admin: true)
        )
        XCTAssertNil(model.errorMessage)
        XCTAssertFalse(model.isSubmitting)
    }

    func testLogoutClearsCoreSessionAndPresentationState() async {
        let client = FakeMusicClient(failLoginAfter: 1)
        let model = LoginViewModel(client: client)
        model.server = "http://localhost"
        model.user = "u"
        model.password = "secret"
        await model.submit()
        XCTAssertNotNil(model.session)
        await model.submit()
        XCTAssertNotNil(model.errorMessage)

        await model.logout()

        let logoutCalls = await client.recordedLogoutCalls()
        XCTAssertEqual(logoutCalls, 1)
        XCTAssertNil(model.session)
        XCTAssertEqual(model.password, "")
        XCTAssertNil(model.errorMessage)
    }

    func testLogoutRejectsEarlierLoginResultThatArrivesLate() async {
        let loginStarted = expectation(description: "login started")
        let client = FakeMusicClient(
            suspendFirstLogin: true,
            onLoginStarted: { loginStarted.fulfill() }
        )
        let model = LoginViewModel(client: client)
        model.password = "secret"
        let submit = Task { await model.submit() }
        await fulfillment(of: [loginStarted], timeout: 1)

        await model.logout()
        await client.resumeLogin()
        await submit.value

        let logoutCalls = await client.recordedLogoutCalls()
        XCTAssertEqual(logoutCalls, 1)
        XCTAssertNil(model.session)
        XCTAssertEqual(model.password, "")
        XCTAssertNil(model.errorMessage)
    }

    func testLogoutPublishesSignedOutStateOnlyAfterCoreSessionIsCleared() async {
        let logoutStarted = expectation(description: "core logout started")
        let client = FakeMusicClient(
            suspendLogout: true,
            onLogoutStarted: { logoutStarted.fulfill() }
        )
        let model = LoginViewModel(client: client)
        model.password = "secret"
        await model.submit()

        let logout = Task { await model.logout() }
        await fulfillment(of: [logoutStarted], timeout: 1)

        XCTAssertNotNil(model.session)
        XCTAssertEqual(model.password, "secret")

        await client.resumeLogout()
        await logout.value
        XCTAssertNil(model.session)
        XCTAssertEqual(model.password, "")
    }

    func testLibraryAppGraphSharesClientAndBrowseModels() {
        let client = FakeMusicClient()
        let graph = LibraryAppGraph(client: client)

        assertSameClient(client, in: graph.browse)
        assertSameClient(client, in: graph.search)
        assertSameClient(client, in: graph.artistDetail)
        assertSameClient(client, in: graph.workflow)
        XCTAssertTrue(workflowLibrary(in: graph.workflow) === graph.browse)
    }

    func testAuthenticatedLibraryGraphOwnersIsolateConsecutiveSessions() async {
        let client = FakeMusicClient()
        let session = SessionValue(server: "http://localhost:4533", user: "same-user", admin: true)
        let first = AuthenticatedLibraryGraphOwner(client: client, session: session)

        await first.graph.browse.reload()
        first.graph.search.setInput("previous search")
        first.graph.artistDetail.load(artistID: "artist-previous")
        await waitUntil { first.graph.artistDetail.detail != nil }
        await first.graph.workflow.importFiles([URL(fileURLWithPath: "/tmp/Previous.flac")])

        let second = AuthenticatedLibraryGraphOwner(client: client, session: session)

        XCTAssertFalse(first.graph === second.graph)
        XCTAssertFalse(first.graph.browse.albums.isEmpty)
        XCTAssertFalse(first.graph.search.input.isEmpty)
        XCTAssertNotNil(first.graph.artistDetail.detail)
        XCTAssertFalse(first.graph.workflow.imports.isEmpty)
        XCTAssertTrue(second.graph.browse.albums.isEmpty)
        XCTAssertEqual(second.graph.search.phase, .idle)
        XCTAssertTrue(second.graph.search.input.isEmpty)
        XCTAssertNil(second.graph.artistDetail.detail)
        XCTAssertTrue(second.graph.workflow.imports.isEmpty)
        assertSameClient(client, in: second.graph.browse)
        assertSameClient(client, in: second.graph.search)
        assertSameClient(client, in: second.graph.artistDetail)
        assertSameClient(client, in: second.graph.workflow)
        XCTAssertTrue(workflowLibrary(in: second.graph.workflow) === second.graph.browse)
    }

    func testLibraryAppGraphConstructsManagementActionsOnlyForAdministrators() {
        XCTAssertEqual(
            LibraryPresentation(width: 1_280, isAdmin: false).managementActions.count,
            0
        )
        XCTAssertEqual(
            LibraryPresentation(width: 1_280, isAdmin: true).managementActions.count,
            3
        )
    }

    func testUploadPublishesCallbackProgress() async {
        let model = UploadViewModel(client: FakeMusicClient())

        await model.upload(localPath: "/tmp/song.flac", libraryKey: "library/song.flac")

        XCTAssertEqual(model.progress, 1)
        XCTAssertNil(model.errorMessage)
        XCTAssertFalse(model.isUploading)
    }

    func testTagEditorSubmitsAnOverlayUpdate() async {
        let model = TagEditorViewModel(client: FakeMusicClient(), trackID: "tr-1")
        model.draft.title = "Retitled"

        await model.save()

        XCTAssertTrue(model.didSave)
        XCTAssertNil(model.errorMessage)
    }

    func testScanStatusRefreshPublishesCoreStatus() async {
        let model = ScanStatusViewModel(client: FakeMusicClient())

        await model.start()
        await model.refresh()

        XCTAssertEqual(model.status, ScanStatus(scanning: true, count: 12))
        XCTAssertNil(model.errorMessage)
    }

    func testWorkbenchShowsUploadSuccessThenScansAndRefreshesLibrary() async {
        let client = FakeMusicClient()
        let library = LibraryBrowseViewModel(client: client)
        let model = LibraryWorkflowViewModel(client: client, library: library)

        await model.importFiles([URL(fileURLWithPath: "/tmp/Song.flac")])

        XCTAssertEqual(model.imports.first?.state, .succeeded)
        XCTAssertEqual(model.imports.first?.track?.title, "Song")
        XCTAssertEqual(model.scanResult?.added, 1)
        XCTAssertEqual(model.scanResult?.changes.first?.track.title, "Song")
        XCTAssertFalse(library.albums.isEmpty)
        let request = await client.recordedAlbumRequest()
        XCTAssertEqual(request?.offset, 0)
        XCTAssertEqual(request?.size, 60)
    }

    func testWorkbenchKeepsUploadSuccessWhenAutomaticScanFails() async {
        let client = FakeMusicClient(scanFails: true)
        let model = LibraryWorkflowViewModel(client: client, library: LibraryBrowseViewModel(client: client))
        await model.importFiles([URL(fileURLWithPath: "/tmp/Song.flac")])
        XCTAssertEqual(model.imports.first?.state, .succeeded)
        XCTAssertNotNil(model.scanError)
    }

    private func waitUntil(
        _ condition: @MainActor () -> Bool,
        file: StaticString = #filePath,
        line: UInt = #line
    ) async {
        for _ in 0 ..< 1_000 where !condition() {
            await Task.yield()
        }
        XCTAssertTrue(condition(), "Condition did not become true", file: file, line: line)
    }
}

private func assertSameClient(
    _ expected: any MusicClientProviding,
    in model: Any,
    file: StaticString = #filePath,
    line: UInt = #line
) {
    let client = Mirror(reflecting: model).descendant("client") as? any MusicClientProviding
    XCTAssertTrue(client as AnyObject === expected as AnyObject, file: file, line: line)
}

private func workflowLibrary(in model: LibraryWorkflowViewModel) -> LibraryBrowseViewModel? {
    Mirror(reflecting: model).descendant("library") as? LibraryBrowseViewModel
}

@MainActor
private final class FakeApplicationActivation: ApplicationActivating {
    var didSetRegularPolicy = false
    var didActivateIgnoringOtherApps = false
    func setRegularActivationPolicy() { didSetRegularPolicy = true }
    func activateIgnoringOtherApps() { didActivateIgnoringOtherApps = true }
}

private actor FakeMusicClient: MusicClientProviding {
    private let album: Album
    private let accessRules: [AccessRule]
    private let searchPageResponse: SearchPage?
    private let scanFails: Bool
    private let loginIsAdmin: Bool
    private let failLoginAfter: Int?
    private let suspendFirstLogin: Bool
    private let onLoginStarted: (@Sendable () -> Void)?
    private let suspendLogout: Bool
    private let onLogoutStarted: (@Sendable () -> Void)?
    private var loginCalls = 0
    private var logoutCalls = 0
    private var loginContinuation: CheckedContinuation<Void, Never>?
    private var logoutContinuation: CheckedContinuation<Void, Never>?
    private(set) var lastSearchPageRequest: SearchPageRequest?
    private var lastAlbumRequest: (offset: UInt32, size: UInt32)?

    init(album: Album = Album(
        id: "al-0",
        name: "",
        artist: nil,
        artistId: nil,
        coverArt: nil,
        songCount: 0,
        duration: 0,
        year: nil,
        genre: nil,
        created: nil
    ), accessRules: [AccessRule] = [], searchPage: SearchPage? = nil,
       scanFails: Bool = false, loginIsAdmin: Bool = true,
       failLoginAfter: Int? = nil, suspendFirstLogin: Bool = false,
       onLoginStarted: (@Sendable () -> Void)? = nil, suspendLogout: Bool = false,
       onLogoutStarted: (@Sendable () -> Void)? = nil) {
        self.album = album
        self.accessRules = accessRules
        self.searchPageResponse = searchPage
        self.scanFails = scanFails
        self.loginIsAdmin = loginIsAdmin
        self.failLoginAfter = failLoginAfter
        self.suspendFirstLogin = suspendFirstLogin
        self.onLoginStarted = onLoginStarted
        self.suspendLogout = suspendLogout
        self.onLogoutStarted = onLogoutStarted
    }

    func login(server: String, user: String, password: String) async throws -> SessionValue {
        loginCalls += 1
        if suspendFirstLogin, loginCalls == 1 {
            onLoginStarted?()
            await withCheckedContinuation { continuation in
                loginContinuation = continuation
            }
        }
        if let failLoginAfter, loginCalls > failLoginAfter {
            throw CocoaError(.fileReadUnknown)
        }
        return SessionValue(server: server, user: user, admin: loginIsAdmin)
    }

    func logout() async {
        logoutCalls += 1
        if suspendLogout {
            onLogoutStarted?()
            await withCheckedContinuation { continuation in
                logoutContinuation = continuation
            }
        }
    }

    func recordedLogoutCalls() -> Int { logoutCalls }
    func recordedSearchPageRequest() -> SearchPageRequest? { lastSearchPageRequest }
    func recordedAlbumRequest() -> (offset: UInt32, size: UInt32)? { lastAlbumRequest }

    func resumeLogin() {
        loginContinuation?.resume()
        loginContinuation = nil
    }

    func resumeLogout() {
        logoutContinuation?.resume()
        logoutContinuation = nil
    }

    func listAlbums(offset: UInt32, size: UInt32) async throws -> [Album] {
        [album]
    }

    func listAlbums(filter: AlbumFilter, offset: UInt32, size: UInt32) async throws -> [Album] {
        lastAlbumRequest = (offset, size)
        return [album]
    }

    func listAccessRules() async throws -> [AccessRule] {
        accessRules
    }

    func search(query: String) async throws -> SearchResult {
        SearchResult(artists: [], albums: [album], tracks: [])
    }

    func searchPage(request: SearchPageRequest) async throws -> SearchPage {
        lastSearchPageRequest = request
        guard let searchPageResponse else { throw CocoaError(.featureUnsupported) }
        return searchPageResponse
    }

    func getArtist(id: String) async throws -> ArtistDetail {
        ArtistDetail(
            artist: Artist(
                id: id,
                name: id,
                sortName: nil,
                coverArt: nil,
                musicBrainzId: nil,
                albumCount: 0
            ),
            albums: []
        )
    }

    func upload(localPath: String, libraryKey: String, progress: UploadProgress) async throws -> Track {
        progress.onProgress(sentBytes: 16, totalBytes: 32)
        progress.onProgress(sentBytes: 32, totalBytes: 32)
        return Track(id: "tr-1", title: "Song", album: nil, albumId: nil, artist: nil, artistId: nil, track: nil, discNumber: nil, year: nil, genre: nil, coverArt: nil, size: 32, contentType: nil, suffix: nil, duration: 0, bitRate: 0, created: nil, path: nil)
    }

    func updateTags(id: String, update: TagUpdate) async throws {}
    func deleteTrack(id: String) async throws {}
    func moveTrack(id: String, key: String) async throws {}
    func startScan() async throws -> ScanStatus { ScanStatus(scanning: true, count: 0) }
    func scanStatus() async throws -> ScanStatus { ScanStatus(scanning: true, count: 12) }
    func scanPrefix(_ prefix: String) async throws -> DetailedScanResult {
        if scanFails { throw CocoaError(.fileReadUnknown) }
        let track = Track(id: "tr-1", title: "Song", album: "Album", albumId: "al-1", artist: "Artist", artistId: "ar-1", track: nil, discNumber: nil, year: nil, genre: nil, coverArt: nil, size: 32, contentType: nil, suffix: "flac", duration: 0, bitRate: 0, created: nil, path: nil)
        return DetailedScanResult(added: 1, updated: 0, deleted: 0, unchanged: 0, changes: [ScanChange(action: .added, objectKey: "library/Song.flac", track: track)], changesTruncated: false)
    }
}
