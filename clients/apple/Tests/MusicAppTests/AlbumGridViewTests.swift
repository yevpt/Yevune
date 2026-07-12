import CoreFFI
import XCTest
@testable import MusicApp

final class AlbumGridViewTests: XCTestCase {
    func testCoverArtIDUsesAlbumCoverArt() {
        let album = albumFixture(coverArt: "covers/ambitions.jpg")

        XCTAssertEqual(coverArtID(for: album), "covers/ambitions.jpg")
    }

    func testCoverArtIDIsNilWhenAlbumHasNoCover() {
        let album = albumFixture(coverArt: nil)

        XCTAssertNil(coverArtID(for: album))
    }

    func testLoadingCoverUsesAlbumCoverArtAsRequestID() async {
        let client = CoverArtClient()
        let album = albumFixture(coverArt: "covers/ambitions.jpg")

        let url = await loadCoverURL(for: album, client: client)

        XCTAssertEqual(client.requestedID, "covers/ambitions.jpg")
        XCTAssertEqual(url, URL(string: "https://example.test/cover.jpg"))
    }
}

private func albumFixture(coverArt: String?) -> Album {
    Album(
        id: "album-1",
        name: "Ambitions",
        artist: "ONE OK ROCK",
        artistId: nil,
        coverArt: coverArt,
        songCount: 1,
        duration: 1,
        year: nil,
        genre: nil,
        created: nil
    )
}

private final class CoverArtClient: MusicClientProviding, @unchecked Sendable {
    private(set) var requestedID: String?

    func login(server: String, user: String, password: String) async throws -> SessionValue {
        SessionValue(server: server, user: user)
    }

    func listAlbums(offset: UInt32, size: UInt32) async throws -> [Album] { [] }

    func search(query: String) async throws -> SearchResult {
        SearchResult(artists: [], albums: [], tracks: [])
    }

    func upload(localPath: String, libraryKey: String, progress: UploadProgress) async throws -> Track {
        throw CocoaError(.featureUnsupported)
    }

    func updateTags(id: String, update: TagUpdate) async throws {}
    func deleteTrack(id: String) async throws {}
    func moveTrack(id: String, key: String) async throws {}
    func startScan() async throws -> ScanStatus { throw CocoaError(.featureUnsupported) }
    func scanStatus() async throws -> ScanStatus { throw CocoaError(.featureUnsupported) }

    func coverArtURL(id: String, size: UInt32?) async throws -> String {
        requestedID = id
        return "https://example.test/cover.jpg"
    }
}
