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

    func testLibraryLoadAndSearchPublishCoreResults() async {
        let album = Album(
            id: "al-1",
            name: "Blue",
            artist: "Band",
            artistId: "ar-1",
            coverArt: nil,
            songCount: 1,
            duration: 120,
            year: nil,
            genre: nil,
            created: nil
        )
        let model = LibraryViewModel(client: FakeMusicClient(album: album))

        await model.load()
        await model.search(query: "Blue")

        XCTAssertEqual(model.albums, [album])
        XCTAssertEqual(model.searchResult?.albums, [album])
        XCTAssertNil(model.errorMessage)
        XCTAssertEqual(model.album(id: "al-1"), album)
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
        model.title = "Retitled"

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
        let library = LibraryViewModel(client: client)
        let model = LibraryWorkflowViewModel(client: client, library: library)

        await model.importFiles([URL(fileURLWithPath: "/tmp/Song.flac")])

        XCTAssertEqual(model.imports.first?.state, .succeeded)
        XCTAssertEqual(model.imports.first?.track?.title, "Song")
        XCTAssertEqual(model.scanResult?.added, 1)
        XCTAssertEqual(model.scanResult?.changes.first?.track.title, "Song")
        XCTAssertFalse(library.albums.isEmpty)
    }

    func testWorkbenchKeepsUploadSuccessWhenAutomaticScanFails() async {
        let client = FakeMusicClient(scanFails: true)
        let model = LibraryWorkflowViewModel(client: client, library: LibraryViewModel(client: client))
        await model.importFiles([URL(fileURLWithPath: "/tmp/Song.flac")])
        XCTAssertEqual(model.imports.first?.state, .succeeded)
        XCTAssertNotNil(model.scanError)
    }
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
    private let scanFails: Bool
    private let loginIsAdmin: Bool

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
    ), accessRules: [AccessRule] = [], scanFails: Bool = false, loginIsAdmin: Bool = true) {
        self.album = album
        self.accessRules = accessRules
        self.scanFails = scanFails
        self.loginIsAdmin = loginIsAdmin
    }

    func login(server: String, user: String, password: String) async throws -> SessionValue {
        SessionValue(server: server, user: user, admin: loginIsAdmin)
    }

    func listAlbums(offset: UInt32, size: UInt32) async throws -> [Album] {
        [album]
    }

    func listAlbums(filter: AlbumFilter, offset: UInt32, size: UInt32) async throws -> [Album] {
        [album]
    }

    func listAccessRules() async throws -> [AccessRule] {
        accessRules
    }

    func search(query: String) async throws -> SearchResult {
        SearchResult(artists: [], albums: [album], tracks: [])
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
