import XCTest
import YevuneCoreFFI
@testable import Yevune

@MainActor
final class FavoriteLibraryViewModelTests: XCTestCase {
    func testLoadPublishesThreeMediaKindsInServerOrder() async {
        let client = FavoriteLibraryClient()
        await client.enqueue(.init(
            artists: [favoriteArtist("ar-2"), favoriteArtist("ar-1")],
            albums: [favoriteAlbum("al-2"), favoriteAlbum("al-1")],
            tracks: [favoriteTrack("tr-2"), favoriteTrack("tr-1")]
        ))
        let model = FavoriteLibraryViewModel(client: client)

        await model.load()

        XCTAssertEqual(model.artists.map(\.id), ["ar-2", "ar-1"])
        XCTAssertEqual(model.albums.map(\.id), ["al-2", "al-1"])
        XCTAssertEqual(model.tracks.map(\.id), ["tr-2", "tr-1"])
        XCTAssertNil(model.initialError)
    }

    func testRefreshFailurePreservesExistingContent() async {
        let client = FavoriteLibraryClient()
        await client.enqueue(.init(artists: [], albums: [], tracks: [favoriteTrack("tr-1")]))
        await client.enqueueError()
        let model = FavoriteLibraryViewModel(client: client)
        await model.load()

        await model.refresh()

        XCTAssertEqual(model.tracks.map(\.id), ["tr-1"])
        XCTAssertNotNil(model.refreshError)
        XCTAssertNil(model.initialError)
    }

    func testForcedRefreshRejectsTheOlderResponse() async throws {
        let client = FavoriteLibraryClient()
        await client.enqueue(
            .init(artists: [], albums: [], tracks: [favoriteTrack("old")]),
            delay: 120_000_000
        )
        await client.enqueue(.init(artists: [], albums: [], tracks: [favoriteTrack("new")]))
        let model = FavoriteLibraryViewModel(client: client)

        let first = Task { await model.load() }
        try await Task.sleep(nanoseconds: 20_000_000)
        await model.refresh()
        await first.value

        XCTAssertEqual(model.tracks.map(\.id), ["new"])
        XCTAssertFalse(model.isLoading)
    }

    func testRemoveOnlyRemovesTheMatchingMediaKind() async {
        let client = FavoriteLibraryClient()
        await client.enqueue(.init(
            artists: [favoriteArtist("same")],
            albums: [favoriteAlbum("same")],
            tracks: [favoriteTrack("same")]
        ))
        let model = FavoriteLibraryViewModel(client: client)
        await model.load()

        model.remove(.track("same"))

        XCTAssertTrue(model.tracks.isEmpty)
        XCTAssertEqual(model.albums.map(\.id), ["same"])
        XCTAssertEqual(model.artists.map(\.id), ["same"])
    }
}

private actor FavoriteLibraryClient: FavoriteLibraryProviding {
    private enum Result {
        case value(StarredCollection, UInt64)
        case failure
    }
    private var results: [Result] = []

    func enqueue(_ value: StarredCollection, delay: UInt64 = 0) {
        results.append(.value(value, delay))
    }

    func enqueueError() {
        results.append(.failure)
    }

    func getStarred() async throws -> StarredCollection {
        let result = results.removeFirst()
        switch result {
        case .value(let value, let delay):
            if delay > 0 { try await Task.sleep(nanoseconds: delay) }
            return value
        case .failure:
            throw CocoaError(.fileReadUnknown)
        }
    }
}

private func favoriteTrack(_ id: String) -> Track {
    Track(
        id: id, title: id, album: "Album", albumId: "al", artist: "Artist", artistId: "ar",
        track: 1, discNumber: 1, year: 2026, genre: nil, coverArt: nil, size: 1,
        contentType: "audio/flac", suffix: "flac", duration: 120, bitRate: 900,
        created: nil, path: nil, starred: "2026-07-18T12:00:00Z", userRating: nil
    )
}

private func favoriteAlbum(_ id: String) -> Album {
    Album(
        id: id, name: id, artist: "Artist", artistId: "ar", coverArt: nil,
        songCount: 1, duration: 120, year: 2026, genre: nil,
        created: nil, starred: "2026-07-18T12:00:00Z", userRating: nil
    )
}

private func favoriteArtist(_ id: String) -> Artist {
    Artist(
        id: id, name: id, sortName: nil, coverArt: nil, musicBrainzId: nil,
        albumCount: 1, starred: "2026-07-18T12:00:00Z", userRating: nil
    )
}
