import XCTest
import YevuneCoreFFI
@testable import Yevune

@MainActor
final class MoveTrackViewModelTests: XCTestCase {
    func testPrefillsDestinationFromTrackPath() {
        let model = MoveTrackViewModel(client: MoveTrackClient(), track: moveTrackFixture())

        XCTAssertEqual(model.destination, "library/Artist/Album/01 Song.flac")
        XCTAssertEqual(model.pathError, "请输入不同的目标路径")
        XCTAssertFalse(model.canSubmit)
        XCTAssertFalse(model.isDirty)

        model.destination = "library/Artist/Album/02 Song.flac"
        XCTAssertTrue(model.isDirty)
    }

    func testRejectsPathOutsideLibraryPrefixWithoutCallingClient() async {
        let client = MoveTrackClient()
        let model = MoveTrackViewModel(client: client, track: moveTrackFixture())
        model.destination = "Artist/Album/01 Song.flac"

        await model.submit()

        XCTAssertEqual(model.pathError, "路径必须以 library/ 开头")
        let calls = await client.calls()
        XCTAssertEqual(calls, [])
    }

    func testRejectsParentTraversalWithoutCallingClient() async {
        let client = MoveTrackClient()
        let model = MoveTrackViewModel(client: client, track: moveTrackFixture())
        model.destination = "library/Artist/../Secret.flac"

        await model.submit()

        XCTAssertEqual(model.pathError, "路径不能包含 ..")
        let calls = await client.calls()
        XCTAssertEqual(calls, [])
    }

    func testRejectsUnchangedTrimmedDestinationWithoutCallingClient() async {
        let client = MoveTrackClient()
        let model = MoveTrackViewModel(client: client, track: moveTrackFixture())
        model.destination = "  library/Artist/Album/01 Song.flac\n"

        await model.submit()

        XCTAssertEqual(model.pathError, "请输入不同的目标路径")
        let calls = await client.calls()
        XCTAssertEqual(calls, [])
    }

    func testSubmissionTrimsDestinationAndRefusesDuplicateWhileInFlight() async {
        let client = MoveTrackClient(suspends: true)
        let model = MoveTrackViewModel(client: client, track: moveTrackFixture())
        model.destination = "  library/Artist/Album/02 Song.flac\n"

        let first = Task { await model.submit() }
        await client.waitForCall()
        XCTAssertTrue(model.isSubmitting)

        await model.submit()
        let callCount = await client.calls().count
        XCTAssertEqual(callCount, 1)

        await client.resolve()
        await first.value
        let calls = await client.calls()
        XCTAssertEqual(
            calls,
            [.init(id: "track:1", key: "library/Artist/Album/02 Song.flac")]
        )
        XCTAssertTrue(model.didMove)
        XCTAssertFalse(model.isSubmitting)
    }

    func testServerFailurePreservesInputAndUsesSafePresentation() async {
        let error = CoreError.Network(
            message: "PUT https://music.test/rest/ext/moveTrack?u=me&t=secret failed"
        )
        let client = MoveTrackClient(error: error)
        let model = MoveTrackViewModel(client: client, track: moveTrackFixture())
        model.destination = "library/Artist/Album/02 Song.flac"

        await model.submit()

        XCTAssertEqual(model.destination, "library/Artist/Album/02 Song.flac")
        XCTAssertFalse(model.didMove)
        XCTAssertEqual(model.errorMessage, LibraryOperationErrorPresentation.message(error))
        XCTAssertFalse(model.errorMessage?.contains("secret") == true)
    }

    func testSuccessfulMovePublishesCompletionAndClearsServerError() async {
        let client = MoveTrackClient()
        let model = MoveTrackViewModel(client: client, track: moveTrackFixture())
        model.destination = "library/Artist/Album/Renamed.flac"

        await model.submit()

        XCTAssertTrue(model.didMove)
        XCTAssertNil(model.errorMessage)
    }
}

private actor MoveTrackClient: MusicClientProviding {
    private let suspends: Bool
    private let error: Error?
    private var recordedCalls: [MoveTrackCall] = []
    private var callWaiters: [CheckedContinuation<Void, Never>] = []
    private var continuation: CheckedContinuation<Void, Never>?

    init(suspends: Bool = false, error: Error? = nil) {
        self.suspends = suspends
        self.error = error
    }

    func moveTrack(id: String, key: String) async throws {
        recordedCalls.append(.init(id: id, key: key))
        callWaiters.forEach { $0.resume() }
        callWaiters.removeAll()
        if suspends {
            await withCheckedContinuation { continuation = $0 }
        }
        if let error { throw error }
    }

    func waitForCall() async {
        guard recordedCalls.isEmpty else { return }
        await withCheckedContinuation { callWaiters.append($0) }
    }

    func resolve() {
        continuation?.resume()
        continuation = nil
    }

    func calls() -> [MoveTrackCall] { recordedCalls }

    func login(server: String, user: String, password: String) async throws -> SessionValue {
        .init(server: server, user: user)
    }
    func logout() async {}
    func listAlbums(offset: UInt32, size: UInt32) async throws -> [Album] { [] }
    func search(query: String) async throws -> SearchResult { .init(artists: [], albums: [], tracks: []) }
    func upload(localPath: String, libraryKey: String, progress: UploadProgress) async throws -> Track {
        throw CocoaError(.featureUnsupported)
    }
    func updateTags(id: String, update: TagUpdate) async throws { throw CocoaError(.featureUnsupported) }
    func deleteTrack(id: String) async throws { throw CocoaError(.featureUnsupported) }
    func startScan() async throws -> ScanStatus { throw CocoaError(.featureUnsupported) }
    func scanStatus() async throws -> ScanStatus { throw CocoaError(.featureUnsupported) }
}

private struct MoveTrackCall: Equatable {
    let id: String
    let key: String
}

private func moveTrackFixture() -> Track {
    Track(
        id: "track:1", title: "Song", album: "Album", albumId: "album:1",
        artist: "Artist", artistId: "artist:1", track: 1, discNumber: 1,
        year: 2026, genre: "Rock", coverArt: nil, size: 0, contentType: nil,
        suffix: "flac", duration: 180, bitRate: 0, created: nil,
        path: "library/Artist/Album/01 Song.flac"
    )
}
