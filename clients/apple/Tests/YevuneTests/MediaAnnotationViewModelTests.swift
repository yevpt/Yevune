import XCTest
import YevuneCoreFFI
@testable import Yevune

@MainActor
final class MediaAnnotationViewModelTests: XCTestCase {
    func testSuccessfulStarWriteRefreshesAuthoritativeTrackState() async {
        let client = AnnotationClientSpy()
        await client.setRefreshedTrack(annotationTrack(starred: "server-time", rating: 4))
        let model = MediaAnnotationViewModel(client: client)
        let target = MediaAnnotationTarget.track("tr-1")
        model.seed(track: annotationTrack(starred: nil, rating: nil))

        await model.setStarred(target: target, starred: true)

        XCTAssertEqual(model.snapshot(for: target), MediaAnnotationSnapshot(isStarred: true, rating: 4))
        XCTAssertNil(model.error(for: target))
        XCTAssertFalse(model.isMutating(target))
        let starWrites = await client.starWrites()
        let trackReads = await client.trackReads()
        XCTAssertEqual(starWrites, [.init(id: "tr-1", type: .track, starred: true)])
        XCTAssertEqual(trackReads, ["tr-1"])
    }

    func testFailedWritePreservesPreviousSnapshotAndPublishesSafeError() async {
        let client = AnnotationClientSpy()
        await client.failWrites(with: CoreError.Server(code: 40, message: "denied"))
        let model = MediaAnnotationViewModel(client: client)
        let target = MediaAnnotationTarget.track("tr-1")
        model.seed(track: annotationTrack(starred: "old", rating: 3))

        await model.setStarred(target: target, starred: false)

        XCTAssertEqual(model.snapshot(for: target), MediaAnnotationSnapshot(isStarred: true, rating: 3))
        XCTAssertEqual(
            model.error(for: target),
            LibraryOperationErrorPresentation.message(CoreError.Server(code: 40, message: "denied"))
        )
        XCTAssertFalse(model.isMutating(target))
        let trackReads = await client.trackReads()
        XCTAssertEqual(trackReads, [])
    }

    func testSameTargetMutationIsSingleFlight() async {
        let client = AnnotationClientSpy()
        await client.setRefreshedTrack(annotationTrack(starred: "new", rating: nil))
        await client.suspendWrites()
        let model = MediaAnnotationViewModel(client: client)
        let target = MediaAnnotationTarget.track("tr-1")
        model.seed(track: annotationTrack(starred: nil, rating: nil))

        let first = Task { await model.setStarred(target: target, starred: true) }
        await Task.yield()
        await model.setStarred(target: target, starred: true)
        let writeCount = await client.starWrites().count
        XCTAssertEqual(writeCount, 1)

        await client.resumeWrites()
        _ = await first.value
        XCTAssertEqual(model.snapshot(for: target)?.isStarred, true)
    }

    func testRatingWriteRefreshesAlbumAndClearErrorRemovesOnlyThatTargetError() async {
        let client = AnnotationClientSpy()
        await client.setRefreshedAlbum(annotationAlbum(starred: nil, rating: 5))
        let model = MediaAnnotationViewModel(client: client)
        let target = MediaAnnotationTarget.album("al-1")
        model.seed(album: annotationAlbum(starred: nil, rating: nil))

        await model.setRating(target: target, rating: 5)

        XCTAssertEqual(model.snapshot(for: target)?.rating, 5)
        let ratingWrites = await client.ratingWrites()
        let albumReads = await client.albumReads()
        XCTAssertEqual(ratingWrites, [.init(id: "al-1", rating: 5)])
        XCTAssertEqual(albumReads, ["al-1"])
    }

    func testStaleMediaSnapshotDoesNotOverwriteRefreshedAnnotationCache() async {
        let client = AnnotationClientSpy()
        await client.setRefreshedTrack(annotationTrack(starred: "new", rating: 5))
        let model = MediaAnnotationViewModel(client: client)
        let target = MediaAnnotationTarget.track("tr-1")
        model.seed(track: annotationTrack(starred: nil, rating: nil))
        await model.setStarred(target: target, starred: true)

        model.seed(track: annotationTrack(starred: nil, rating: nil))

        XCTAssertEqual(model.snapshot(for: target), .init(isStarred: true, rating: 5))
    }
}

private struct StarWrite: Equatable, Sendable {
    let id: String
    let type: AnnotationItemType
    let starred: Bool
}

private struct RatingWrite: Equatable, Sendable {
    let id: String
    let rating: UInt8?
}

private actor AnnotationClientSpy: MediaAnnotationProviding {
    private var stars: [StarWrite] = []
    private var ratings: [RatingWrite] = []
    private var readTracks: [String] = []
    private var readAlbums: [String] = []
    private var readArtists: [String] = []
    private var refreshedTrack = annotationTrack(starred: nil, rating: nil)
    private var refreshedAlbum = annotationAlbum(starred: nil, rating: nil)
    private var refreshedArtist = Artist(
        id: "ar-1", name: "Artist", sortName: nil, coverArt: nil,
        musicBrainzId: nil, albumCount: 1, starred: nil, userRating: nil
    )
    private var writeError: Error?
    private var suspended = false
    private var continuation: CheckedContinuation<Void, Never>?

    func setRefreshedTrack(_ track: Track) { refreshedTrack = track }
    func setRefreshedAlbum(_ album: Album) { refreshedAlbum = album }
    func failWrites(with error: Error) { writeError = error }
    func suspendWrites() { suspended = true }
    func resumeWrites() {
        suspended = false
        continuation?.resume()
        continuation = nil
    }

    func starWrites() -> [StarWrite] { stars }
    func ratingWrites() -> [RatingWrite] { ratings }
    func trackReads() -> [String] { readTracks }
    func albumReads() -> [String] { readAlbums }

    func setStarred(id: String, itemType: AnnotationItemType, starred: Bool) async throws {
        stars.append(.init(id: id, type: itemType, starred: starred))
        if suspended {
            await withCheckedContinuation { continuation = $0 }
        }
        if let writeError { throw writeError }
    }

    func setRating(id: String, rating: UInt8?) async throws {
        ratings.append(.init(id: id, rating: rating))
        if let writeError { throw writeError }
    }

    func getSong(id: String) async throws -> Track {
        readTracks.append(id)
        return refreshedTrack
    }

    func getAlbum(id: String) async throws -> AlbumDetail {
        readAlbums.append(id)
        return AlbumDetail(album: refreshedAlbum, tracks: [])
    }

    func getArtist(id: String) async throws -> ArtistDetail {
        readArtists.append(id)
        return ArtistDetail(artist: refreshedArtist, albums: [])
    }
}

private func annotationTrack(starred: String?, rating: UInt8?) -> Track {
    Track(
        id: "tr-1", title: "Song", album: "Album", albumId: "al-1",
        artist: "Artist", artistId: "ar-1", track: 1, discNumber: 1,
        year: 2026, genre: "Rock", coverArt: nil, size: 42,
        contentType: "audio/flac", suffix: "flac", duration: 180,
        bitRate: 900, created: nil, path: nil, starred: starred, userRating: rating
    )
}

private func annotationAlbum(starred: String?, rating: UInt8?) -> Album {
    Album(
        id: "al-1", name: "Album", artist: "Artist", artistId: "ar-1",
        coverArt: nil, songCount: 1, duration: 180, year: 2026, genre: "Rock",
        created: nil, starred: starred, userRating: rating
    )
}
