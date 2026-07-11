import XCTest
import CoreFFI
@testable import MusicApp

@MainActor
final class LoginViewModelTests: XCTestCase {
    func testProductionBridgeConformsToViewModelProtocol() {
        let client: any MusicClientProviding = CoreMusicClient()
        XCTAssertNotNil(client as AnyObject)
    }

    func testSubmitPublishesAuthenticatedSession() async {
        let client = FakeMusicClient()
        let model = LoginViewModel(client: client)
        model.server = "http://music.local:4533"
        model.user = "admin"
        model.password = "secret"

        await model.submit()

        XCTAssertEqual(model.session, .init(server: "http://music.local:4533", user: "admin"))
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
    }

    func testUploadPublishesCallbackProgress() async {
        let model = UploadViewModel(client: FakeMusicClient())

        await model.upload(localPath: "/tmp/song.flac", libraryKey: "library/song.flac")

        XCTAssertEqual(model.progress, 1)
        XCTAssertNil(model.errorMessage)
        XCTAssertFalse(model.isUploading)
    }
}

private actor FakeMusicClient: MusicClientProviding {
    private let album: Album

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
    )) {
        self.album = album
    }

    func login(server: String, user: String, password: String) async throws -> SessionValue {
        SessionValue(server: server, user: user)
    }

    func listAlbums(offset: UInt32, size: UInt32) async throws -> [Album] {
        [album]
    }

    func search(query: String) async throws -> SearchResult {
        SearchResult(artists: [], albums: [album], tracks: [])
    }

    func upload(localPath: String, libraryKey: String, progress: UploadProgress) async throws {
        progress.onProgress(sentBytes: 16, totalBytes: 32)
        progress.onProgress(sentBytes: 32, totalBytes: 32)
    }
}
