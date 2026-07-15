import Foundation
import YevuneCoreFFI
import XCTest
@testable import Yevune

@MainActor
final class PlaybackControllerTests: XCTestCase {
    func testPlayLoadsRequestedTrackAndPublishesQueue() async {
        let engine = RecordingPlaybackEngine()
        let resolver = RecordingMediaResolver()
        let controller = PlaybackController(resolver: resolver, engine: engine)

        await controller.play(
            tracks: [playbackTrack("1"), playbackTrack("2")],
            startingAt: 1
        )

        XCTAssertEqual(controller.currentTrack?.id, "2")
        XCTAssertEqual(controller.queueEntries.map(\.track.id), ["1", "2"])
        XCTAssertEqual(resolver.resolvedTrackIDs, ["2"])
        XCTAssertEqual(engine.loadedURLs.map(\.lastPathComponent), ["2"])
        XCTAssertEqual(controller.coverURL?.lastPathComponent, "cover-2")
    }

    func testPreviousAndNextLoadAdjacentEntriesDirectly() async {
        let engine = RecordingPlaybackEngine()
        let controller = PlaybackController(resolver: RecordingMediaResolver(), engine: engine)
        await controller.play(tracks: [playbackTrack("1"), playbackTrack("2")], startingAt: 1)

        await controller.previous()
        await controller.next()

        XCTAssertEqual(engine.loadedURLs.map(\.lastPathComponent), ["2", "1", "2"])
    }

    func testEngineEventsPublishPlaybackStateAndProgress() async {
        let engine = RecordingPlaybackEngine()
        let controller = PlaybackController(resolver: RecordingMediaResolver(), engine: engine)
        await controller.play(tracks: [playbackTrack("1")], startingAt: 0)

        engine.send(.state(.buffering))
        engine.send(.time(elapsed: 12, duration: 180))

        XCTAssertEqual(controller.engineState, .buffering)
        XCTAssertEqual(controller.elapsed, 12)
        XCTAssertEqual(controller.duration, 180)
    }

    func testQueueInsertionIntentsPublishEntriesWithoutLoading() async {
        let engine = RecordingPlaybackEngine()
        let controller = PlaybackController(resolver: RecordingMediaResolver(), engine: engine)
        await controller.play(tracks: [playbackTrack("1"), playbackTrack("3")], startingAt: 0)

        controller.playNext(playbackTrack("2"))
        controller.addToQueue(playbackTrack("4"))

        XCTAssertEqual(controller.queueEntries.map(\.track.id), ["1", "2", "3", "4"])
        XCTAssertEqual(engine.loadedURLs.map(\.lastPathComponent), ["1"])
    }

    func testQueueEditIntentsPublishRemovedMovedAndClearedEntries() async {
        let controller = PlaybackController(
            resolver: RecordingMediaResolver(),
            engine: RecordingPlaybackEngine()
        )
        await controller.play(
            tracks: [playbackTrack("1"), playbackTrack("2"), playbackTrack("3"), playbackTrack("4")],
            startingAt: 1
        )

        controller.moveQueueEntry(from: 3, to: 2)
        controller.removeFromQueue(id: controller.queueEntries[0].id)
        controller.clearUpcoming()

        XCTAssertEqual(controller.queueEntries.map(\.track.id), ["2"])
        XCTAssertEqual(controller.currentTrack?.id, "2")
    }

    func testShuffleKeepsCurrentTrackAndRepeatCycleIsOffAllOne() async {
        let controller = PlaybackController(
            resolver: RecordingMediaResolver(),
            engine: RecordingPlaybackEngine(),
            shuffle: { Array($0.reversed()) }
        )
        await controller.play(
            tracks: [playbackTrack("1"), playbackTrack("2"), playbackTrack("3")],
            startingAt: 0
        )

        controller.setShuffled(true)
        XCTAssertEqual(controller.currentTrack?.id, "1")
        XCTAssertEqual(controller.queueEntries.map(\.track.id), ["1", "3", "2"])
        XCTAssertEqual(controller.repeatMode, .off)
        controller.cycleRepeatMode()
        XCTAssertEqual(controller.repeatMode, .all)
        controller.cycleRepeatMode()
        XCTAssertEqual(controller.repeatMode, .one)
    }

    func testPlayNowReplacesQueueWithSingleTrack() async {
        let engine = RecordingPlaybackEngine()
        let controller = PlaybackController(resolver: RecordingMediaResolver(), engine: engine)
        await controller.play(tracks: [playbackTrack("1"), playbackTrack("2")], startingAt: 0)

        await controller.playNow(playbackTrack("3"))

        XCTAssertEqual(controller.queueEntries.map(\.track.id), ["3"])
        XCTAssertEqual(controller.currentTrack?.id, "3")
        XCTAssertEqual(engine.loadedURLs.map(\.lastPathComponent), ["1", "3"])
    }

    func testPlaybackControlIntentsForwardToEngineAndPublishLocalValues() async {
        let engine = RecordingPlaybackEngine()
        let controller = PlaybackController(resolver: RecordingMediaResolver(), engine: engine)
        await controller.play(tracks: [playbackTrack("1")], startingAt: 0)

        engine.send(.state(.playing))
        controller.togglePlayPause()
        engine.send(.state(.paused))
        controller.togglePlayPause()
        controller.seek(to: 42)
        controller.setVolume(0.25)
        controller.toggleMuted()

        XCTAssertEqual(engine.pauseCalls, 1)
        XCTAssertEqual(engine.playCalls, 1)
        XCTAssertEqual(engine.seekValues, [42])
        XCTAssertEqual(engine.volumeValues, [0.25])
        XCTAssertEqual(engine.mutedValues, [true])
        XCTAssertEqual(controller.volume, 0.25)
        XCTAssertTrue(controller.isMuted)
    }

    func testNaturalEndAdvancesOnMainActorWithoutSleeping() async {
        let advanced = expectation(description: "next track resolved")
        let engine = RecordingPlaybackEngine()
        let resolver = RecordingMediaResolver(onResolve: { id in
            if id == "2" { advanced.fulfill() }
        })
        let controller = PlaybackController(resolver: resolver, engine: engine)
        await controller.play(tracks: [playbackTrack("1"), playbackTrack("2")], startingAt: 0)

        engine.send(.ended)
        await fulfillment(of: [advanced], timeout: 1)

        XCTAssertEqual(controller.currentTrack?.id, "2")
        XCTAssertEqual(engine.loadedURLs.map(\.lastPathComponent), ["1", "2"])
    }

    func testNaturalEndQueuedBeforeShutdownCannotRestartPlayback() async {
        let engine = RecordingPlaybackEngine()
        let resolver = RecordingMediaResolver()
        let controller = PlaybackController(resolver: resolver, engine: engine)
        await controller.play(tracks: [playbackTrack("1"), playbackTrack("2")], startingAt: 0)

        engine.send(.ended)
        controller.shutdown()
        await Task.yield()

        XCTAssertEqual(resolver.resolvedTrackIDs, ["1"])
        XCTAssertEqual(engine.loadedURLs.map(\.lastPathComponent), ["1"])
        XCTAssertEqual(engine.stopCalls, 1)
    }

    func testNaturalEndQueuedBeforeNewPlayCannotAdvanceReplacementQueue() async {
        let engine = RecordingPlaybackEngine()
        let resolver = RecordingMediaResolver(yieldingTrackIDs: ["3"])
        let controller = PlaybackController(resolver: resolver, engine: engine)
        await controller.play(tracks: [playbackTrack("1"), playbackTrack("2")], startingAt: 0)

        engine.send(.ended)
        await controller.play(tracks: [playbackTrack("3"), playbackTrack("4")], startingAt: 0)

        XCTAssertEqual(controller.currentTrack?.id, "3")
        XCTAssertEqual(resolver.resolvedTrackIDs, ["1", "3"])
        XCTAssertEqual(engine.loadedURLs.map(\.lastPathComponent), ["1", "3"])
    }

    func testSupersededResolutionCannotReplaceNewerPlayback() async {
        let firstRequestStarted = expectation(description: "first resolution started")
        let engine = RecordingPlaybackEngine()
        let resolver = SuspendingFirstMediaResolver(onFirstRequest: {
            firstRequestStarted.fulfill()
        })
        let controller = PlaybackController(resolver: resolver, engine: engine)

        let firstPlay = Task { await controller.play(tracks: [playbackTrack("1")], startingAt: 0) }
        await fulfillment(of: [firstRequestStarted], timeout: 1)
        await controller.play(tracks: [playbackTrack("2")], startingAt: 0)
        resolver.resumeFirstRequest()
        await firstPlay.value

        XCTAssertEqual(controller.currentTrack?.id, "2")
        XCTAssertEqual(engine.loadedURLs.map(\.lastPathComponent), ["2"])
    }

    func testOldEngineEventsCannotAffectReplacementQueueWhileResolutionIsPending() async {
        let replacementRequestStarted = expectation(description: "replacement resolution started")
        let engine = RecordingPlaybackEngine()
        let resolver = SuspendingMediaResolver(
            suspendedTrackID: "3",
            onSuspendedRequest: { replacementRequestStarted.fulfill() }
        )
        let controller = PlaybackController(resolver: resolver, engine: engine)
        await controller.play(tracks: [playbackTrack("1"), playbackTrack("2")], startingAt: 0)

        let replacementPlay = Task {
            await controller.play(
                tracks: [playbackTrack("3"), playbackTrack("4")],
                startingAt: 0
            )
        }
        await fulfillment(of: [replacementRequestStarted], timeout: 1)

        engine.send(.state(.playing))
        engine.send(.time(elapsed: 12, duration: 180))
        engine.send(.failed(message: "https://secret.invalid/old"))
        engine.send(.ended)
        await Task.yield()

        XCTAssertEqual(controller.queueEntries.map(\.track.id), ["3", "4"])
        XCTAssertEqual(controller.currentTrack?.id, "3")
        XCTAssertEqual(controller.engineState, .idle)
        XCTAssertEqual(controller.elapsed, 0)
        XCTAssertEqual(controller.duration, 0)
        XCTAssertNil(controller.errorMessage)
        XCTAssertEqual(resolver.resolvedTrackIDs, ["1", "3"])

        resolver.resumeSuspendedRequest()
        await replacementPlay.value

        XCTAssertEqual(engine.loadedURLs.map(\.lastPathComponent), ["1", "3"])
        XCTAssertEqual(resolver.resolvedTrackIDs, ["1", "3"])
    }

    func testResolutionFailurePublishesSafeMessageWithoutLoadingURL() async {
        let leakedURL = "https://secret.invalid/token?credential=private"
        let engine = RecordingPlaybackEngine()
        let controller = PlaybackController(
            resolver: RecordingMediaResolver(error: ResolverFailure(message: leakedURL)),
            engine: engine
        )

        await controller.play(tracks: [playbackTrack("1")], startingAt: 0)

        XCTAssertEqual(controller.errorMessage, "无法播放「1」")
        XCTAssertFalse(controller.errorMessage?.contains(leakedURL) ?? true)
        XCTAssertTrue(engine.loadedURLs.isEmpty)
    }

    func testEngineFailurePublishesSafeMessageWithoutForwardingURL() async {
        let leakedURL = "https://secret.invalid/token?credential=private"
        let engine = RecordingPlaybackEngine()
        let controller = PlaybackController(resolver: RecordingMediaResolver(), engine: engine)
        await controller.play(tracks: [playbackTrack("1")], startingAt: 0)

        engine.send(.failed(message: leakedURL))
        await controller.waitForPendingTransitionForTesting()
        engine.send(.failed(message: leakedURL))
        await controller.waitForPendingTransitionForTesting()

        XCTAssertEqual(controller.errorMessage, "无法播放 1，已跳到下一首")
        XCTAssertFalse(controller.errorMessage?.contains(leakedURL) ?? true)
    }

    func testFailureRefreshesURLOnceThenSkipsBadEntry() async {
        let engine = RecordingPlaybackEngine()
        let resolver = RecordingMediaResolver()
        let controller = PlaybackController(resolver: resolver, engine: engine)
        await controller.play(tracks: [playbackTrack("bad"), playbackTrack("good")], startingAt: 0)

        engine.send(.failed(message: "expired"))
        await controller.waitForPendingTransitionForTesting()
        engine.send(.failed(message: "still bad"))
        await controller.waitForPendingTransitionForTesting()

        XCTAssertEqual(resolver.resolvedTrackIDs, ["bad", "bad", "good"])
        XCTAssertEqual(controller.currentTrack?.id, "good")
        XCTAssertEqual(controller.errorMessage, "无法播放 bad，已跳到下一首")
    }

    func testFailedEntryIsNotRetriedForeverWhenRepeatAllWraps() async {
        let engine = RecordingPlaybackEngine()
        let resolver = RecordingMediaResolver()
        let controller = PlaybackController(resolver: resolver, engine: engine)
        await controller.play(tracks: [playbackTrack("a"), playbackTrack("b")], startingAt: 0)
        controller.cycleRepeatMode()

        for _ in 0..<4 {
            engine.send(.failed(message: "broken"))
            await controller.waitForPendingTransitionForTesting()
        }

        XCTAssertEqual(resolver.resolvedTrackIDs, ["a", "a", "b", "b"])
        XCTAssertEqual(engine.stopCalls, 1)
    }

    func testNaturalEndSkipsFailedEntryWhenRepeatAllWraps() async {
        let engine = RecordingPlaybackEngine()
        let resolver = RecordingMediaResolver()
        let controller = PlaybackController(resolver: resolver, engine: engine)
        await controller.play(tracks: [playbackTrack("a"), playbackTrack("b")], startingAt: 0)
        controller.cycleRepeatMode()

        engine.send(.failed(message: "a1"))
        await controller.waitForPendingTransitionForTesting()
        engine.send(.failed(message: "a2"))
        await controller.waitForPendingTransitionForTesting()
        engine.send(.ended)
        await controller.waitForPendingTransitionForTesting()

        XCTAssertEqual(resolver.resolvedTrackIDs, ["a", "a", "b", "b"])
        XCTAssertEqual(controller.currentTrack?.id, "b")
    }

    func testRefreshResolutionFailureSkipsBadEntry() async {
        let engine = RecordingPlaybackEngine()
        let resolver = FailingRefreshMediaResolver(failingTrackID: "a")
        let controller = PlaybackController(resolver: resolver, engine: engine)
        await controller.play(tracks: [playbackTrack("a"), playbackTrack("b")], startingAt: 0)

        engine.send(.failed(message: "expired"))
        await controller.waitForPendingTransitionForTesting()

        XCTAssertEqual(resolver.resolvedTrackIDs, ["a", "a", "b"])
        XCTAssertEqual(controller.currentTrack?.id, "b")
        XCTAssertEqual(controller.errorMessage, "无法播放 a，已跳到下一首")
    }

    func testRemovingCurrentLoadsSuccessorAfterStoppingOldMedia() async {
        let engine = RecordingPlaybackEngine()
        let resolver = RecordingMediaResolver()
        let controller = PlaybackController(resolver: resolver, engine: engine)
        await controller.play(tracks: [playbackTrack("a"), playbackTrack("b")], startingAt: 0)

        controller.removeFromQueue(id: controller.queueEntries[0].id)

        XCTAssertEqual(engine.stopCalls, 1)
        XCTAssertEqual(controller.currentTrack?.id, "b")
        await controller.waitForPendingTransitionForTesting()
        XCTAssertEqual(resolver.resolvedTrackIDs, ["a", "b"])
        XCTAssertEqual(engine.loadedURLs.map(\.lastPathComponent), ["a", "b"])
    }

    func testRemovingOnlyCurrentStopsAndEmptiesQueue() async {
        let engine = RecordingPlaybackEngine()
        let controller = PlaybackController(resolver: RecordingMediaResolver(), engine: engine)
        await controller.play(tracks: [playbackTrack("a")], startingAt: 0)

        controller.removeFromQueue(id: controller.queueEntries[0].id)

        XCTAssertEqual(engine.stopCalls, 1)
        XCTAssertTrue(controller.queueEntries.isEmpty)
        XCTAssertNil(controller.currentTrack)
    }

    func testRemovingCurrentRejectsSuspendedResolverResult() async {
        let requestStarted = expectation(description: "current resolution started")
        let engine = RecordingPlaybackEngine()
        let resolver = SuspendingMediaResolver(
            suspendedTrackID: "a",
            onSuspendedRequest: { requestStarted.fulfill() }
        )
        let controller = PlaybackController(resolver: resolver, engine: engine)
        let initialPlay = Task {
            await controller.play(
                tracks: [playbackTrack("a"), playbackTrack("b")],
                startingAt: 0
            )
        }
        await fulfillment(of: [requestStarted], timeout: 1)

        controller.removeFromQueue(id: controller.queueEntries[0].id)
        await controller.waitForPendingTransitionForTesting()
        resolver.resumeSuspendedRequest()
        await initialPlay.value

        XCTAssertEqual(controller.queueEntries.map(\.track.id), ["b"])
        XCTAssertEqual(controller.currentTrack?.id, "b")
        XCTAssertEqual(resolver.resolvedTrackIDs, ["a", "b"])
        XCTAssertEqual(engine.loadedURLs.map(\.lastPathComponent), ["b"])
    }

    func testDirectQueueSelectionClearsFailureStateForThatEntry() async {
        let engine = RecordingPlaybackEngine()
        let resolver = RecordingMediaResolver()
        let controller = PlaybackController(resolver: resolver, engine: engine)
        await controller.play(tracks: [playbackTrack("bad")], startingAt: 0)

        engine.send(.failed(message: "first"))
        await controller.waitForPendingTransitionForTesting()
        engine.send(.failed(message: "second"))
        await controller.waitForPendingTransitionForTesting()

        await controller.playQueueEntry(id: controller.queueEntries[0].id)
        engine.send(.failed(message: "manual retry"))
        await controller.waitForPendingTransitionForTesting()

        XCTAssertEqual(resolver.resolvedTrackIDs, ["bad", "bad", "bad", "bad"])
    }

    func testDuplicateTrackInstancesKeepIndependentRetryCounts() async {
        let engine = RecordingPlaybackEngine()
        let resolver = RecordingMediaResolver()
        let controller = PlaybackController(resolver: resolver, engine: engine)
        let duplicate = playbackTrack("same")
        await controller.play(tracks: [duplicate, duplicate], startingAt: 0)

        for _ in 0..<3 {
            engine.send(.failed(message: "broken"))
            await controller.waitForPendingTransitionForTesting()
        }

        XCTAssertEqual(resolver.resolvedTrackIDs, ["same", "same", "same", "same"])
        XCTAssertEqual(controller.queueEntries.map(\.track.id), ["same", "same"])
        XCTAssertEqual(controller.currentTrack?.id, "same")
    }

    func testRemovingFailedEntryDoesNotMakeRemainingQueueLookFullyFailed() async {
        let engine = RecordingPlaybackEngine()
        let resolver = RecordingMediaResolver()
        let controller = PlaybackController(resolver: resolver, engine: engine)
        await controller.play(
            tracks: [playbackTrack("a"), playbackTrack("b"), playbackTrack("c")],
            startingAt: 0
        )

        engine.send(.failed(message: "a1"))
        await controller.waitForPendingTransitionForTesting()
        engine.send(.failed(message: "a2"))
        await controller.waitForPendingTransitionForTesting()
        controller.removeFromQueue(id: controller.queueEntries[0].id)
        engine.send(.failed(message: "b1"))
        await controller.waitForPendingTransitionForTesting()
        engine.send(.failed(message: "b2"))
        await controller.waitForPendingTransitionForTesting()

        XCTAssertEqual(resolver.resolvedTrackIDs, ["a", "a", "b", "b", "c"])
        XCTAssertEqual(controller.currentTrack?.id, "c")
    }

    func testFailureQueuedBeforeManualNextCannotAdvanceNewSession() async {
        let engine = RecordingPlaybackEngine()
        let resolver = RecordingMediaResolver()
        let controller = PlaybackController(resolver: resolver, engine: engine)
        await controller.play(tracks: [playbackTrack("a"), playbackTrack("b")], startingAt: 0)

        engine.send(.failed(message: "broken"))
        await controller.next()
        await controller.waitForPendingTransitionForTesting()

        XCTAssertEqual(resolver.resolvedTrackIDs, ["a", "b"])
        XCTAssertEqual(controller.currentTrack?.id, "b")
        XCTAssertEqual(engine.loadedURLs.map(\.lastPathComponent), ["a", "b"])
    }

    func testFailureQueuedBeforeNewPlayCannotAdvanceReplacementQueue() async {
        let engine = RecordingPlaybackEngine()
        let resolver = RecordingMediaResolver()
        let controller = PlaybackController(resolver: resolver, engine: engine)
        await controller.play(tracks: [playbackTrack("a"), playbackTrack("b")], startingAt: 0)

        engine.send(.failed(message: "broken"))
        await controller.play(tracks: [playbackTrack("c"), playbackTrack("d")], startingAt: 0)
        await controller.waitForPendingTransitionForTesting()

        XCTAssertEqual(resolver.resolvedTrackIDs, ["a", "c"])
        XCTAssertEqual(controller.queueEntries.map(\.track.id), ["c", "d"])
        XCTAssertEqual(controller.currentTrack?.id, "c")
        XCTAssertEqual(engine.loadedURLs.map(\.lastPathComponent), ["a", "c"])
    }

    func testFailureQueuedBeforeShutdownCannotRestartPlayback() async {
        let engine = RecordingPlaybackEngine()
        let resolver = RecordingMediaResolver()
        let controller = PlaybackController(resolver: resolver, engine: engine)
        await controller.play(tracks: [playbackTrack("a")], startingAt: 0)

        engine.send(.failed(message: "broken"))
        controller.shutdown()
        await controller.waitForPendingTransitionForTesting()

        XCTAssertEqual(resolver.resolvedTrackIDs, ["a"])
        XCTAssertTrue(controller.queueEntries.isEmpty)
        XCTAssertTrue(engine.loadedURLs.map(\.lastPathComponent) == ["a"])
        XCTAssertEqual(engine.stopCalls, 1)
    }

    func testShutdownStopsEngineAndClearsTransientPlaybackState() async {
        let engine = RecordingPlaybackEngine()
        let controller = PlaybackController(resolver: RecordingMediaResolver(), engine: engine)
        await controller.play(tracks: [playbackTrack("1")], startingAt: 0)
        engine.send(.state(.playing))
        engine.send(.time(elapsed: 12, duration: 180))

        controller.shutdown()

        XCTAssertEqual(engine.stopCalls, 1)
        XCTAssertEqual(controller.engineState, .idle)
        XCTAssertEqual(controller.elapsed, 0)
        XCTAssertEqual(controller.duration, 0)
        XCTAssertNil(controller.coverURL)
    }

    func testShutdownResetsSessionIgnoresLateEventsAndAllowsExplicitRestart() async {
        let engine = RecordingPlaybackEngine()
        let controller = PlaybackController(resolver: RecordingMediaResolver(), engine: engine)
        await controller.play(
            tracks: [playbackTrack("1"), playbackTrack("2")],
            startingAt: 0
        )
        engine.send(.state(.playing))
        engine.send(.time(elapsed: 12, duration: 180))
        engine.send(.failed(message: "old failure"))

        controller.shutdown()
        engine.send(.state(.playing))
        engine.send(.time(elapsed: 99, duration: 999))
        engine.send(.failed(message: "late failure"))
        engine.send(.ended)
        await Task.yield()

        XCTAssertTrue(controller.queueEntries.isEmpty)
        XCTAssertNil(controller.currentTrack)
        XCTAssertNil(controller.coverURL)
        XCTAssertEqual(controller.engineState, .idle)
        XCTAssertEqual(controller.elapsed, 0)
        XCTAssertEqual(controller.duration, 0)
        XCTAssertNil(controller.errorMessage)
        XCTAssertEqual(engine.loadedURLs.map(\.lastPathComponent), ["1"])

        await controller.playNow(playbackTrack("3"))
        engine.send(.state(.playing))
        engine.send(.time(elapsed: 3, duration: 30))

        XCTAssertEqual(controller.queueEntries.map(\.track.id), ["3"])
        XCTAssertEqual(controller.currentTrack?.id, "3")
        XCTAssertEqual(controller.engineState, .playing)
        XCTAssertEqual(controller.elapsed, 3)
        XCTAssertEqual(controller.duration, 30)
        XCTAssertEqual(engine.loadedURLs.map(\.lastPathComponent), ["1", "3"])
    }
}

@MainActor
final class PlaybackMediaResolverTests: XCTestCase {
    func testResolveRequestsAuthenticatedStreamAndOptionalCoverURLs() async throws {
        let client = RecordingResolverClient(
            stream: "https://music.invalid/stream/1?token=secret",
            cover: "https://music.invalid/cover/1?token=secret"
        )
        let resolver = MusicClientMediaResolver(client: client)

        let media = try await resolver.resolve(track: playbackTrack("1"))

        XCTAssertEqual(client.streamTrackIDs, ["1"])
        XCTAssertEqual(client.coverRequests, [.init(id: "cover:1", size: 600)])
        XCTAssertEqual(media.streamURL.path, "/stream/1")
        XCTAssertEqual(media.coverURL?.path, "/cover/1")
    }

    func testResolveRejectsInvalidStreamURLWithSafeDescription() async {
        let invalidURL = "http://["
        let resolver = MusicClientMediaResolver(
            client: RecordingResolverClient(stream: invalidURL, cover: nil)
        )

        do {
            _ = try await resolver.resolve(track: playbackTrack("1"))
            XCTFail("Expected invalid URL failure")
        } catch {
            XCTAssertEqual(error.localizedDescription, "服务器返回了无效的播放地址")
            XCTAssertFalse(error.localizedDescription.contains(invalidURL))
        }
    }

    func testResolveIgnoresCoverFailure() async throws {
        let resolver = MusicClientMediaResolver(
            client: RecordingResolverClient(
                stream: "https://music.invalid/stream/1",
                coverError: CocoaError(.fileReadUnknown)
            )
        )

        let media = try await resolver.resolve(track: playbackTrack("1"))

        XCTAssertNil(media.coverURL)
    }
}

@MainActor
private final class RecordingPlaybackEngine: PlaybackEngine {
    var onEvent: ((PlaybackEngineEvent) -> Void)?
    private(set) var loadedURLs: [URL] = []
    private(set) var playCalls = 0
    private(set) var pauseCalls = 0
    private(set) var seekValues: [TimeInterval] = []
    private(set) var volumeValues: [Float] = []
    private(set) var mutedValues: [Bool] = []
    private(set) var stopCalls = 0

    func load(url: URL, autoplay: Bool) { loadedURLs.append(url) }
    func play() { playCalls += 1 }
    func pause() { pauseCalls += 1 }
    func seek(to seconds: TimeInterval) { seekValues.append(seconds) }
    func setVolume(_ volume: Float) { volumeValues.append(volume) }
    func setMuted(_ muted: Bool) { mutedValues.append(muted) }
    func stop() { stopCalls += 1 }
    func send(_ event: PlaybackEngineEvent) { onEvent?(event) }
}

@MainActor
private final class RecordingMediaResolver: PlaybackMediaResolving {
    private(set) var resolvedTrackIDs: [String] = []
    private let error: Error?
    private let onResolve: ((String) -> Void)?
    private let yieldingTrackIDs: Set<String>

    init(
        error: Error? = nil,
        onResolve: ((String) -> Void)? = nil,
        yieldingTrackIDs: Set<String> = []
    ) {
        self.error = error
        self.onResolve = onResolve
        self.yieldingTrackIDs = yieldingTrackIDs
    }

    func resolve(track: Track) async throws -> ResolvedPlaybackMedia {
        resolvedTrackIDs.append(track.id)
        if yieldingTrackIDs.contains(track.id) {
            await Task.yield()
        }
        if let error { throw error }
        onResolve?(track.id)
        return ResolvedPlaybackMedia(
            streamURL: URL(string: "https://example.invalid/\(track.id)")!,
            coverURL: URL(string: "https://example.invalid/cover-\(track.id)")
        )
    }
}

@MainActor
private final class FailingRefreshMediaResolver: PlaybackMediaResolving {
    private(set) var resolvedTrackIDs: [String] = []
    private let failingTrackID: String
    private var attempts: [String: Int] = [:]

    init(failingTrackID: String) {
        self.failingTrackID = failingTrackID
    }

    func resolve(track: Track) async throws -> ResolvedPlaybackMedia {
        resolvedTrackIDs.append(track.id)
        attempts[track.id, default: 0] += 1
        if track.id == failingTrackID, attempts[track.id] == 2 {
            throw ResolverFailure(message: "refresh failed")
        }
        return ResolvedPlaybackMedia(
            streamURL: URL(string: "https://example.invalid/\(track.id)")!,
            coverURL: nil
        )
    }
}

@MainActor
private final class SuspendingFirstMediaResolver: PlaybackMediaResolving {
    private let onFirstRequest: () -> Void
    private var firstContinuation: CheckedContinuation<ResolvedPlaybackMedia, Error>?

    init(onFirstRequest: @escaping () -> Void) {
        self.onFirstRequest = onFirstRequest
    }

    func resolve(track: Track) async throws -> ResolvedPlaybackMedia {
        if track.id != "1" {
            return media(for: track.id)
        }

        onFirstRequest()
        return try await withCheckedThrowingContinuation { continuation in
            firstContinuation = continuation
        }
    }

    func resumeFirstRequest() {
        firstContinuation?.resume(returning: media(for: "1"))
        firstContinuation = nil
    }

    private func media(for id: String) -> ResolvedPlaybackMedia {
        ResolvedPlaybackMedia(
            streamURL: URL(string: "https://example.invalid/\(id)")!,
            coverURL: nil
        )
    }
}

@MainActor
private final class SuspendingMediaResolver: PlaybackMediaResolving {
    private(set) var resolvedTrackIDs: [String] = []
    private let suspendedTrackID: String
    private let onSuspendedRequest: () -> Void
    private var continuation: CheckedContinuation<ResolvedPlaybackMedia, Error>?

    init(suspendedTrackID: String, onSuspendedRequest: @escaping () -> Void) {
        self.suspendedTrackID = suspendedTrackID
        self.onSuspendedRequest = onSuspendedRequest
    }

    func resolve(track: Track) async throws -> ResolvedPlaybackMedia {
        resolvedTrackIDs.append(track.id)
        guard track.id == suspendedTrackID else { return media(for: track.id) }
        onSuspendedRequest()
        return try await withCheckedThrowingContinuation { continuation in
            self.continuation = continuation
        }
    }

    func resumeSuspendedRequest() {
        continuation?.resume(returning: media(for: suspendedTrackID))
        continuation = nil
    }

    private func media(for id: String) -> ResolvedPlaybackMedia {
        ResolvedPlaybackMedia(
            streamURL: URL(string: "https://example.invalid/\(id)")!,
            coverURL: URL(string: "https://example.invalid/cover-\(id)")
        )
    }
}

private struct ResolverFailure: LocalizedError {
    let message: String
    var errorDescription: String? { message }
}

private final class RecordingResolverClient: MusicClientProviding, @unchecked Sendable {
    struct CoverRequest: Equatable {
        let id: String
        let size: UInt32?
    }

    let stream: String
    let cover: String?
    let coverError: Error?
    private(set) var streamTrackIDs: [String] = []
    private(set) var coverRequests: [CoverRequest] = []

    init(stream: String, cover: String? = nil, coverError: Error? = nil) {
        self.stream = stream
        self.cover = cover
        self.coverError = coverError
    }

    func login(server: String, user: String, password: String) async throws -> SessionValue {
        throw CocoaError(.featureUnsupported)
    }

    func listAlbums(offset: UInt32, size: UInt32) async throws -> [Album] { [] }
    func search(query: String) async throws -> SearchResult { .init(artists: [], albums: [], tracks: []) }
    func upload(localPath: String, libraryKey: String, progress: UploadProgress) async throws -> Track {
        throw CocoaError(.featureUnsupported)
    }

    func updateTags(id: String, update: TagUpdate) async throws { throw CocoaError(.featureUnsupported) }
    func deleteTrack(id: String) async throws { throw CocoaError(.featureUnsupported) }
    func moveTrack(id: String, key: String) async throws { throw CocoaError(.featureUnsupported) }
    func startScan() async throws -> ScanStatus { throw CocoaError(.featureUnsupported) }
    func scanStatus() async throws -> ScanStatus { throw CocoaError(.featureUnsupported) }

    func streamURL(trackID: String) async throws -> String {
        streamTrackIDs.append(trackID)
        return stream
    }

    func coverArtURL(id: String, size: UInt32?) async throws -> String {
        coverRequests.append(.init(id: id, size: size))
        if let coverError { throw coverError }
        return cover ?? ""
    }
}
