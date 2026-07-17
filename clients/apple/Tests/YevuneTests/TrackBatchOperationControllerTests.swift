import Foundation
import YevuneCoreFFI
import XCTest
@testable import Yevune

@MainActor
final class TrackBatchOperationControllerTests: XCTestCase {
    func testMediaModelKeepsBatchControllerAcrossDetailReconstruction() async {
        let client = SuspendedBatchClient()
        let media = MediaViewModel(client: client)
        let firstConsumer = media.makeBatchController()
        firstConsumer.reset(for: "album-a")
        let run = Task {
            await firstConsumer.run(tracks: batchTracks(3), action: .delete, onFinished: {})
        }

        await client.waitForCalls(1)
        let reconstructedConsumer = media.makeBatchController()

        XCTAssertTrue(firstConsumer === reconstructedConsumer)
        XCTAssertTrue(reconstructedConsumer.isRunning)
        XCTAssertEqual(reconstructedConsumer.currentTrackID, "track:1")
        XCTAssertEqual(reconstructedConsumer.results.map(\.state), [.pending, .pending, .pending])

        reconstructedConsumer.stop()
        let stoppedThroughReconstructedConsumer = firstConsumer.stopRequested
        if !stoppedThroughReconstructedConsumer {
            firstConsumer.stop()
        }
        XCTAssertTrue(stoppedThroughReconstructedConsumer)
        await client.resolveCall(0)
        await run.value

        XCTAssertEqual(reconstructedConsumer.results.map(\.state), [.succeeded, .skipped, .skipped])
        XCTAssertFalse(reconstructedConsumer.isRunning)
    }

    func testUpdatesRunOneAtATimeAndRefreshExactlyOnce() async {
        let client = SuspendedBatchClient()
        let refresh = BatchRefreshRecorder()
        let model = TrackBatchOperationController(client: client)

        let run = Task {
            await model.run(
                tracks: [batchTrack("1"), batchTrack("2")],
                action: .update(batchUpdateFixture()),
                onFinished: { await refresh.record("album-a") }
            )
        }

        await client.waitForCalls(1)
        let firstCallCount = await client.callCount()
        let firstMaximumInFlight = await client.maximumInFlightCount()
        XCTAssertEqual(firstCallCount, 1)
        XCTAssertEqual(firstMaximumInFlight, 1)
        await client.resolveCall(0)
        await client.waitForCalls(2)
        let finalMaximumInFlight = await client.maximumInFlightCount()
        XCTAssertEqual(finalMaximumInFlight, 1)
        await client.resolveCall(1)
        await run.value

        let refreshedAlbums = await refresh.albums()
        XCTAssertEqual(refreshedAlbums, ["album-a"])
        XCTAssertEqual(model.completedCount, 2)
        XCTAssertEqual(model.totalCount, 2)
        XCTAssertEqual(model.results.map(\.state), [.succeeded, .succeeded])
    }

    func testStopFinishesCurrentAndSkipsRemaining() async {
        let client = SuspendedBatchClient()
        let refresh = BatchRefreshRecorder()
        let model = TrackBatchOperationController(client: client)
        let run = Task {
            await model.run(
                tracks: batchTracks(3),
                action: .delete,
                onFinished: { await refresh.record("album-a") }
            )
        }

        await client.waitForCalls(1)
        model.stop()
        await client.resolveCall(0)
        await run.value

        let callCount = await client.callCount()
        let refreshedAlbums = await refresh.albums()
        XCTAssertEqual(callCount, 1)
        XCTAssertEqual(model.results.map(\.state), [.succeeded, .skipped, .skipped])
        XCTAssertEqual(refreshedAlbums, ["album-a"])
        XCTAssertFalse(model.isRunning)
    }

    func testFailureContinuesInOrderAndUsesSafeErrorPresentation() async {
        let client = SuspendedBatchClient()
        let model = TrackBatchOperationController(client: client)
        let run = Task {
            await model.run(tracks: batchTracks(3), action: .delete, onFinished: {})
        }

        await client.waitForCalls(1)
        await client.resolveCall(0)
        await client.waitForCalls(2)
        await client.rejectCall(
            1,
            error: CoreError.Network(
                message: "GET https://music.test/rest/deleteTrack?u=me&t=secret failed"
            )
        )
        await client.waitForCalls(3)
        await client.resolveCall(2)
        await run.value

        let calledTrackIDs = await client.calledTrackIDs()
        XCTAssertEqual(calledTrackIDs, ["track:1", "track:2", "track:3"])
        guard case let .failed(message) = model.results[1].state else {
            return XCTFail("expected the second track to fail")
        }
        XCTAssertFalse(message.contains("https://music.test"))
        XCTAssertFalse(message.contains("secret"))
        XCTAssertEqual(model.results[2].state, .succeeded)
    }

    func testRetryFailedSendsOnlyFailedIDsInOriginalOrder() async {
        let client = SuspendedBatchClient()
        let refresh = BatchRefreshRecorder()
        let model = TrackBatchOperationController(client: client)
        let firstRun = Task {
            await model.run(tracks: batchTracks(4), action: .delete) {
                await refresh.record("first")
            }
        }

        await client.waitForCalls(1)
        await client.rejectCall(0)
        await client.waitForCalls(2)
        await client.resolveCall(1)
        await client.waitForCalls(3)
        await client.rejectCall(2)
        await client.waitForCalls(4)
        await client.resolveCall(3)
        await firstRun.value

        let retry = Task {
            await model.retryFailed { await refresh.record("retry") }
        }
        await client.waitForCalls(5)
        await client.resolveCall(4)
        await client.waitForCalls(6)
        await client.resolveCall(5)
        await retry.value

        let calledTrackIDs = await client.calledTrackIDs()
        let refreshedAlbums = await refresh.albums()
        XCTAssertEqual(
            calledTrackIDs,
            ["track:1", "track:2", "track:3", "track:4", "track:1", "track:3"]
        )
        XCTAssertEqual(model.results.map(\.id), ["track:1", "track:3"])
        XCTAssertEqual(model.results.map(\.state), [.succeeded, .succeeded])
        XCTAssertEqual(refreshedAlbums, ["first", "retry"])
    }

    func testRunRefusesReentryWhileRequestIsInFlight() async {
        let client = SuspendedBatchClient()
        let refresh = BatchRefreshRecorder()
        let model = TrackBatchOperationController(client: client)
        let firstRun = Task {
            await model.run(tracks: batchTracks(2), action: .delete) {
                await refresh.record("first")
            }
        }

        await client.waitForCalls(1)
        await model.run(tracks: [batchTrack("other")], action: .delete) {
            await refresh.record("reentry")
        }
        let callCountDuringReentry = await client.callCount()
        XCTAssertEqual(callCountDuringReentry, 1)

        await client.resolveCall(0)
        await client.waitForCalls(2)
        await client.resolveCall(1)
        await firstRun.value

        let calledTrackIDs = await client.calledTrackIDs()
        let refreshedAlbums = await refresh.albums()
        XCTAssertEqual(calledTrackIDs, ["track:1", "track:2"])
        XCTAssertEqual(refreshedAlbums, ["first"])
    }

    func testResetWaitsForCurrentRequestThenClearsOldResultsWithoutPublishingIntoNewAlbum() async {
        let client = SuspendedBatchClient()
        let refresh = BatchRefreshRecorder()
        let model = TrackBatchOperationController(client: client)
        model.reset(for: "album-a")
        let run = Task {
            await model.run(tracks: batchTracks(2), action: .delete) {
                await refresh.record("album-a")
            }
        }

        await client.waitForCalls(1)
        XCTAssertEqual(model.results.map(\.state), [.pending, .pending])
        XCTAssertEqual(model.resultAlbumID, "album-a")
        model.reset(for: "album-b")

        XCTAssertEqual(model.albumID, "album-b")
        XCTAssertEqual(model.resultAlbumID, "album-a")
        XCTAssertEqual(
            model.results.map(\.state), [.pending, .pending],
            "album A results remain visible until its in-flight request finishes"
        )

        await client.resolveCall(0)
        await run.value

        let callCount = await client.callCount()
        let refreshedAlbums = await refresh.albums()
        XCTAssertEqual(callCount, 1)
        XCTAssertTrue(model.results.isEmpty)
        XCTAssertNil(model.resultAlbumID)
        XCTAssertNil(model.currentTrackID)
        XCTAssertFalse(model.isRunning)
        XCTAssertEqual(refreshedAlbums, ["album-a"])
    }

    func testValidatedBatchDraftProducesUpdateAction() async throws {
        var draft = BatchTagDraft()
        draft.genre = .clear
        let update = try XCTUnwrap(draft.makeUpdate())
        let client = SuspendedBatchClient()
        let model = TrackBatchOperationController(client: client)
        let run = Task {
            await model.run(tracks: [batchTrack("1")], action: .update(update), onFinished: {})
        }

        await client.waitForCalls(1)
        await client.resolveCall(0)
        await run.value

        let actions = await client.calledActions()
        XCTAssertEqual(actions, [.update(update)])
    }
}

private actor BatchRefreshRecorder {
    private var recordedAlbums: [String] = []

    func record(_ albumID: String) {
        recordedAlbums.append(albumID)
    }

    func albums() -> [String] { recordedAlbums }
}

private actor SuspendedBatchClient: MusicClientProviding {
    private struct Call {
        let trackID: String
        let action: BatchClientAction
        var continuation: CheckedContinuation<Void, Error>?
    }

    private var calls: [Call] = []
    private var waiters: [(Int, CheckedContinuation<Void, Never>)] = []
    private var inFlightCount = 0
    private var maximumInFlight = 0

    func updateTags(id: String, update: TagUpdate) async throws {
        try await suspend(trackID: id, action: .update(update))
    }

    func deleteTrack(id: String) async throws {
        try await suspend(trackID: id, action: .delete)
    }

    func waitForCalls(_ count: Int) async {
        guard calls.count < count else { return }
        await withCheckedContinuation { waiters.append((count, $0)) }
    }

    func resolveCall(_ index: Int) {
        finishCall(index, result: .success(()))
    }

    func rejectCall(_ index: Int, error: Error = BatchTestError.rejected) {
        finishCall(index, result: .failure(error))
    }

    func callCount() -> Int { calls.count }
    func maximumInFlightCount() -> Int { maximumInFlight }
    func calledTrackIDs() -> [String] { calls.map(\.trackID) }
    func calledActions() -> [BatchClientAction] { calls.map(\.action) }

    private func suspend(trackID: String, action: BatchClientAction) async throws {
        try await withCheckedThrowingContinuation { continuation in
            calls.append(Call(trackID: trackID, action: action, continuation: continuation))
            inFlightCount += 1
            maximumInFlight = max(maximumInFlight, inFlightCount)
            let ready = waiters.filter { $0.0 <= calls.count }
            waiters.removeAll { $0.0 <= calls.count }
            ready.forEach { $0.1.resume() }
        }
    }

    private func finishCall(_ index: Int, result: Result<Void, Error>) {
        guard let continuation = calls[index].continuation else { return }
        calls[index].continuation = nil
        inFlightCount -= 1
        continuation.resume(with: result)
    }

    func login(server: String, user: String, password: String) async throws -> SessionValue {
        .init(server: server, user: user)
    }
    func logout() async {}
    func listAlbums(offset: UInt32, size: UInt32) async throws -> [Album] { [] }
    func search(query: String) async throws -> SearchResult { .init(artists: [], albums: [], tracks: []) }
    func upload(localPath: String, libraryKey: String, progress: UploadProgress) async throws -> Track {
        throw CocoaError(.featureUnsupported)
    }
    func moveTrack(id: String, key: String) async throws { throw CocoaError(.featureUnsupported) }
    func startScan() async throws -> ScanStatus { throw CocoaError(.featureUnsupported) }
    func scanStatus() async throws -> ScanStatus { throw CocoaError(.featureUnsupported) }
}

private enum BatchClientAction: Equatable {
    case update(TagUpdate)
    case delete
}

private enum BatchTestError: LocalizedError {
    case rejected

    var errorDescription: String? { "request rejected" }
}

private func batchTracks(_ count: Int) -> [Track] {
    (1...count).map { batchTrack(String($0)) }
}

private func batchTrack(_ id: String) -> Track {
    Track(
        id: "track:\(id)", title: "Track \(id)", album: "Album", albumId: "album-a",
        artist: "Artist", artistId: "artist:1", track: nil, discNumber: nil, year: nil,
        genre: nil, coverArt: nil, size: 0, contentType: nil, suffix: "flac", duration: 0,
        bitRate: 0, created: nil, path: nil
    )
}

private func batchUpdateFixture() -> TagUpdate {
    TagUpdate(
        title: nil, album: nil, artist: nil, genre: "Shared", year: nil, track: nil,
        discNumber: nil, clearFields: []
    )
}
