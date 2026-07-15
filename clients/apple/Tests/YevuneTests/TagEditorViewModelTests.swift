import YevuneCoreFFI
import XCTest
@testable import Yevune

@MainActor
final class TagEditorViewModelTests: XCTestCase {
    func testInitWithTrackPrefillsEditableFieldsAndMoveKey() {
        let model = TagEditorViewModel(client: RecordingTrackClient(), track: trackFixture())

        XCTAssertEqual(model.title, "Song")
        XCTAssertEqual(model.album, "Album")
        XCTAssertEqual(model.artist, "Artist")
        XCTAssertEqual(model.genre, "Rock")
        XCTAssertEqual(model.year, "2024")
        XCTAssertEqual(model.track, "3")
        XCTAssertEqual(model.discNumber, "2")
        XCTAssertEqual(model.moveKey, "library/Artist/Album/03 Song.flac")
    }

    func testSaveOnlySubmitsNonEmptyFields() async {
        let client = RecordingTrackClient()
        let model = TagEditorViewModel(client: client, trackID: "track:1")
        model.title = "Retitled"
        model.year = "2025"

        await model.save()

        XCTAssertEqual(client.tagUpdates, [
            .init(id: "track:1", update: TagUpdate(title: "Retitled", album: nil, artist: nil, genre: nil, year: 2025, track: nil, discNumber: nil)),
        ])
        XCTAssertTrue(model.didSave)
        XCTAssertNil(model.errorMessage)
    }

    func testDeleteAndMoveUseTrackIDAndPrefilledKey() async {
        let client = RecordingTrackClient()
        let model = TagEditorViewModel(client: client, track: trackFixture())

        await model.move()
        await model.delete()

        XCTAssertEqual(client.moves, [.init(id: "track:1", key: "library/Artist/Album/03 Song.flac")])
        XCTAssertEqual(client.deletedTrackIDs, ["track:1"])
        XCTAssertTrue(model.didMove)
        XCTAssertTrue(model.didDelete)
    }

    func testBatchTagUpdateContinuesAfterFailuresAndRefreshes() async {
        let client = RecordingTrackClient(failingTrackIDs: ["track:2"])
        let model = MediaViewModel(client: client)
        let update = TagUpdate(title: "Shared", album: nil, artist: nil, genre: nil, year: nil, track: nil, discNumber: nil)

        await model.updateTags(ids: ["track:1", "track:2"], update: update, album: albumFixture())

        XCTAssertEqual(client.tagUpdates.map(\.id), ["track:1", "track:2"])
        XCTAssertEqual(client.albumLoads, ["album:1"])
        XCTAssertEqual(model.errorMessage, "1 项操作失败")
    }

    func testBatchDeleteContinuesAfterFailuresAndRefreshes() async {
        let client = RecordingTrackClient(failingTrackIDs: ["track:2"])
        let model = MediaViewModel(client: client)

        await model.deleteTracks(ids: ["track:1", "track:2"], album: albumFixture())

        XCTAssertEqual(client.deletedTrackIDs, ["track:1", "track:2"])
        XCTAssertEqual(client.albumLoads, ["album:1"])
        XCTAssertEqual(model.errorMessage, "1 项操作失败")
    }
}

@MainActor
private func trackFixture() -> Track {
    Track(
        id: "track:1", title: "Song", album: "Album", albumId: "album:1", artist: "Artist", artistId: "artist:1",
        track: 3, discNumber: 2, year: 2024, genre: "Rock", coverArt: nil, size: 0, contentType: nil,
        suffix: "flac", duration: 0, bitRate: 0, created: nil, path: "library/Artist/Album/03 Song.flac"
    )
}

@MainActor
private func albumFixture() -> Album {
    Album(id: "album:1", name: "Album", artist: "Artist", artistId: "artist:1", coverArt: nil, songCount: 2, duration: 0, year: 2024, genre: "Rock", created: nil)
}

private final class RecordingTrackClient: MusicClientProviding, @unchecked Sendable {
    let failingTrackIDs: Set<String>
    private(set) var tagUpdates: [TagUpdateCall] = []
    private(set) var deletedTrackIDs: [String] = []
    private(set) var moves: [MoveCall] = []
    private(set) var albumLoads: [String] = []

    init(failingTrackIDs: Set<String> = []) {
        self.failingTrackIDs = failingTrackIDs
    }

    func login(server: String, user: String, password: String) async throws -> SessionValue { .init(server: server, user: user) }
    func logout() async {}
    func listAlbums(offset: UInt32, size: UInt32) async throws -> [Album] { [] }
    func search(query: String) async throws -> SearchResult { .init(artists: [], albums: [], tracks: []) }
    func upload(localPath: String, libraryKey: String, progress: UploadProgress) async throws -> Track { throw CocoaError(.featureUnsupported) }
    func startScan() async throws -> ScanStatus { throw CocoaError(.featureUnsupported) }
    func scanStatus() async throws -> ScanStatus { throw CocoaError(.featureUnsupported) }

    func updateTags(id: String, update: TagUpdate) async throws {
        tagUpdates.append(.init(id: id, update: update))
        if failingTrackIDs.contains(id) { throw CocoaError(.fileReadUnknown) }
    }

    func deleteTrack(id: String) async throws {
        deletedTrackIDs.append(id)
        if failingTrackIDs.contains(id) { throw CocoaError(.fileReadUnknown) }
    }

    func moveTrack(id: String, key: String) async throws {
        moves.append(.init(id: id, key: key))
        if failingTrackIDs.contains(id) { throw CocoaError(.fileReadUnknown) }
    }

    func getAlbum(id: String) async throws -> AlbumDetail {
        albumLoads.append(id)
        return AlbumDetail(album: await albumFixture(), tracks: [])
    }
}

private struct TagUpdateCall: Equatable {
    let id: String
    let update: TagUpdate
}

private struct MoveCall: Equatable {
    let id: String
    let key: String
}
