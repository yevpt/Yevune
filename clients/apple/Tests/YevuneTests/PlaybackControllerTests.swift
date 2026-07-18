import AppKit
import Foundation
import YevuneCoreFFI
import XCTest
@testable import Yevune

@MainActor
final class PlaybackControllerTests: XCTestCase {
    func testControllerLoadsArtworkForCurrentSystemMetadata() async {
        let artwork = NSImage(size: NSSize(width: 10, height: 10))
        let loader = FakeArtworkLoader(image: artwork)
        let system = RecordingSystemMediaCoordinator()
        let controller = PlaybackController(
            resolver: RecordingMediaResolver(), engine: RecordingPlaybackEngine(),
            systemMedia: system, artworkLoader: loader
        )

        await controller.play(tracks: [playbackTrack("1")], startingAt: 0)
        await controller.waitForPendingArtworkForTesting()

        XCTAssertEqual(loader.loadedURLs.map(\.lastPathComponent), ["cover-1"])
        XCTAssertTrue(system.lastArtwork === artwork)
        XCTAssertTrue(controller.artwork === artwork)
    }

    func testControllerPublishesTrackDurationBeforeEngineTimingArrives() async {
        let system = RecordingSystemMediaCoordinator()
        let controller = PlaybackController(
            resolver: RecordingMediaResolver(),
            engine: RecordingPlaybackEngine(),
            systemMedia: system
        )

        await controller.play(
            tracks: [playbackTrack("1", title: "Song", duration: 180)],
            startingAt: 0
        )

        XCTAssertEqual(system.lastTrack?.title, "Song")
        XCTAssertEqual(system.lastDuration, 180)
    }

    func testControllerRegistersSystemHandlersOncePerPlaybackLifecycle() async {
        let system = RecordingSystemMediaCoordinator()
        let controller = PlaybackController(
            resolver: RecordingMediaResolver(),
            engine: RecordingPlaybackEngine(),
            systemMedia: system
        )

        await controller.playNow(playbackTrack("1"))
        await controller.playNow(playbackTrack("2"))

        XCTAssertEqual(system.registerCalls, 1)

        controller.shutdown()
        await controller.playNow(playbackTrack("3"))
        XCTAssertEqual(system.registerCalls, 2)
    }

    func testLateRemoteCommandAfterShutdownCannotReachEngine() async {
        let engine = RecordingPlaybackEngine()
        let system = RecordingSystemMediaCoordinator()
        let controller = PlaybackController(
            resolver: RecordingMediaResolver(), engine: engine, systemMedia: system
        )
        await controller.playNow(playbackTrack("1"))
        let staleHandlers = system.handlers

        controller.shutdown()
        staleHandlers?.play()
        staleHandlers?.pause()
        staleHandlers?.seek(42)

        XCTAssertEqual(engine.playCalls, 0)
        XCTAssertEqual(engine.pauseCalls, 0)
        XCTAssertTrue(engine.seekValues.isEmpty)

        await controller.play(
            tracks: [playbackTrack("2"), playbackTrack("3")],
            startingAt: 0
        )
        staleHandlers?.play()
        staleHandlers?.pause()
        staleHandlers?.next()
        staleHandlers?.seek(99)
        await Task.yield()

        XCTAssertEqual(controller.currentTrack?.id, "2")
        XCTAssertEqual(engine.playCalls, 0)
        XCTAssertEqual(engine.pauseCalls, 0)
        XCTAssertTrue(engine.seekValues.isEmpty)
    }

    func testSupersededArtworkCannotReplaceCurrentTrackArtwork() async {
        let firstStarted = expectation(description: "first artwork request started")
        let oldArtwork = NSImage(size: NSSize(width: 10, height: 10))
        let currentArtwork = NSImage(size: NSSize(width: 20, height: 20))
        let loader = SupersedingArtworkLoader(
            currentImage: currentArtwork,
            onFirstRequest: { firstStarted.fulfill() }
        )
        let system = RecordingSystemMediaCoordinator()
        let controller = PlaybackController(
            resolver: RecordingMediaResolver(), engine: RecordingPlaybackEngine(),
            systemMedia: system, artworkLoader: loader
        )

        await controller.playNow(playbackTrack("1"))
        await fulfillment(of: [firstStarted], timeout: 1)
        await controller.playNow(playbackTrack("2"))
        await controller.waitForPendingArtworkForTesting()
        loader.resumeFirst(with: oldArtwork)
        await Task.yield()

        XCTAssertTrue(system.lastArtwork === currentArtwork)
        XCTAssertTrue(controller.artwork === currentArtwork)
    }

    func testArtworkArrivingAfterShutdownCannotRestoreSystemMetadata() async {
        let requestStarted = expectation(description: "artwork request started")
        let loader = SupersedingArtworkLoader(
            currentImage: NSImage(size: NSSize(width: 20, height: 20)),
            onFirstRequest: { requestStarted.fulfill() }
        )
        let system = RecordingSystemMediaCoordinator()
        let controller = PlaybackController(
            resolver: RecordingMediaResolver(), engine: RecordingPlaybackEngine(),
            systemMedia: system, artworkLoader: loader
        )

        await controller.playNow(playbackTrack("1"))
        await fulfillment(of: [requestStarted], timeout: 1)
        controller.shutdown()
        loader.resumeFirst(with: NSImage(size: NSSize(width: 10, height: 10)))
        await Task.yield()

        XCTAssertNil(system.lastTrack)
        XCTAssertNil(system.lastArtwork)
        XCTAssertNil(controller.artwork)
    }

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

    func testCurrentQueueEntryIdentityDistinguishesDuplicateTracks() async {
        let duplicate = playbackTrack("duplicate")
        let controller = PlaybackController(
            resolver: RecordingMediaResolver(),
            engine: RecordingPlaybackEngine()
        )

        await controller.play(tracks: [duplicate, duplicate], startingAt: 1)

        XCTAssertEqual(controller.currentQueueEntryID, controller.queueEntries[1].id)
        XCTAssertNotEqual(controller.currentQueueEntryID, controller.queueEntries[0].id)
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

    func testSuccessfulMediaLoadReportsPlaybackStartWithoutBlockingPlayback() async {
        let reporter = RecordingPlaybackReporter()
        let engine = RecordingPlaybackEngine()
        let controller = PlaybackController(
            resolver: RecordingMediaResolver(),
            engine: engine,
            reporter: reporter
        )

        await controller.playNow(playbackTrack("1"))
        await controller.waitForPendingReportsForTesting()

        XCTAssertEqual(reporter.events, [.init(trackID: "1", submission: false)])
        XCTAssertEqual(engine.loadedURLs.map(\.lastPathComponent), ["1"])
    }

    func testActualListeningReachingThresholdSubmitsExactlyOnce() async {
        let reporter = RecordingPlaybackReporter()
        let engine = RecordingPlaybackEngine()
        let controller = PlaybackController(
            resolver: RecordingMediaResolver(),
            engine: engine,
            reporter: reporter
        )
        await controller.playNow(playbackTrack("1", duration: 120))
        await controller.waitForPendingReportsForTesting()

        engine.send(.state(.playing))
        for elapsed in stride(from: 0.0, through: 65.0, by: 5.0) {
            engine.send(.time(elapsed: elapsed, duration: 120))
        }
        await controller.waitForPendingReportsForTesting()

        engine.send(.ended)
        await controller.waitForPendingTransitionForTesting()
        await controller.waitForPendingReportsForTesting()

        XCTAssertEqual(
            reporter.events,
            [
                .init(trackID: "1", submission: false),
                .init(trackID: "1", submission: true),
            ]
        )
    }

    func testPlaybackHistoryThresholdIgnoresShortPreviewsAndCapsLongTracksAtFourMinutes() {
        XCTAssertNil(PlaybackHistorySession.submissionThreshold(duration: 59))
        XCTAssertEqual(PlaybackHistorySession.submissionThreshold(duration: 60), 30)
        XCTAssertEqual(PlaybackHistorySession.submissionThreshold(duration: 1_000), 240)
    }

    func testMediaURLRetryDoesNotStartASecondPlaybackHistoryInstance() async {
        let reporter = RecordingPlaybackReporter()
        let engine = RecordingPlaybackEngine()
        let controller = PlaybackController(
            resolver: RecordingMediaResolver(),
            engine: engine,
            reporter: reporter
        )
        await controller.playNow(playbackTrack("1"))
        await controller.waitForPendingReportsForTesting()

        engine.send(.failed(message: "expired"))
        await controller.waitForPendingTransitionForTesting()
        await controller.waitForPendingReportsForTesting()

        XCTAssertEqual(reporter.events.filter { !$0.submission }.count, 1)
        XCTAssertEqual(engine.loadedURLs.map(\.lastPathComponent), ["1", "1"])
    }

    func testSeekingAcrossThresholdDoesNotCountAsListeningButNaturalEndStillSubmits() async {
        let reporter = RecordingPlaybackReporter()
        let engine = RecordingPlaybackEngine()
        let controller = PlaybackController(
            resolver: RecordingMediaResolver(),
            engine: engine,
            reporter: reporter
        )
        await controller.playNow(playbackTrack("1", duration: 120))
        await controller.waitForPendingReportsForTesting()

        engine.send(.state(.playing))
        engine.send(.time(elapsed: 0, duration: 120))
        engine.send(.time(elapsed: 90, duration: 120))
        await controller.waitForPendingReportsForTesting()
        XCTAssertEqual(reporter.events.filter(\.submission).count, 0)

        engine.send(.ended)
        await controller.waitForPendingReportsForTesting()
        XCTAssertEqual(reporter.events.filter(\.submission).count, 1)
    }

    func testScrobbleFailureDoesNotBecomePlaybackFailure() async {
        let reporter = RecordingPlaybackReporter(
            error: URLError(.networkConnectionLost)
        )
        let engine = RecordingPlaybackEngine()
        let controller = PlaybackController(
            resolver: RecordingMediaResolver(),
            engine: engine,
            reporter: reporter
        )

        await controller.playNow(playbackTrack("1", duration: 10))
        await controller.waitForPendingReportsForTesting()
        engine.send(.ended)
        await controller.waitForPendingReportsForTesting()

        XCTAssertNil(controller.errorMessage)
        XCTAssertEqual(engine.loadedURLs.map(\.lastPathComponent), ["1"])
        XCTAssertEqual(reporter.events.map { $0.submission }, [false, true])
    }

    func testCompletedSubmissionWaitsForTheStartReportWithoutBlockingQueueAdvance() async {
        let startBegan = expectation(description: "start report began")
        let reporter = SuspendingStartPlaybackReporter(onStart: { startBegan.fulfill() })
        let engine = RecordingPlaybackEngine()
        let controller = PlaybackController(
            resolver: RecordingMediaResolver(),
            engine: engine,
            reporter: reporter
        )
        await controller.playNow(playbackTrack("1", duration: 10))
        await fulfillment(of: [startBegan], timeout: 1)

        engine.send(.ended)
        await controller.waitForPendingTransitionForTesting()
        await Task.yield()

        XCTAssertEqual(reporter.events, [.init(trackID: "1", submission: false)])
        XCTAssertEqual(controller.engineState, .paused)

        reporter.resumeStart()
        await controller.waitForPendingReportsForTesting()
        XCTAssertEqual(
            reporter.events,
            [
                .init(trackID: "1", submission: false),
                .init(trackID: "1", submission: true),
            ]
        )
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

    func testNaturalEndAtQueueTailDetachesOldMediaAndReloadsCurrentEntryForReplay() async {
        let engine = RecordingPlaybackEngine()
        let resolver = RecordingMediaResolver()
        let system = RecordingSystemMediaCoordinator()
        let controller = PlaybackController(
            resolver: resolver, engine: engine, systemMedia: system
        )
        await controller.play(
            tracks: [playbackTrack("last", title: "Last", duration: 4)],
            startingAt: 0
        )
        engine.send(.state(.playing))
        engine.send(.time(elapsed: 4, duration: 4))
        let oldMediaCallback = try! XCTUnwrap(engine.onEvent)

        engine.send(.ended)
        await controller.waitForPendingTransitionForTesting()

        XCTAssertEqual(controller.currentTrack?.id, "last")
        XCTAssertEqual(controller.engineState, .paused)
        XCTAssertEqual(controller.elapsed, 0)
        XCTAssertEqual(controller.duration, 4)
        XCTAssertEqual(engine.stopCalls, 1)
        XCTAssertNil(engine.onEvent)
        XCTAssertEqual(system.lastTrack?.id, "last")
        XCTAssertEqual(system.lastState, .paused)
        XCTAssertEqual(system.lastElapsed, 0)
        XCTAssertEqual(system.lastDuration, 4)

        oldMediaCallback(.state(.buffering))
        oldMediaCallback(.time(elapsed: 4, duration: 4))
        oldMediaCallback(.failed(message: "late failure"))
        oldMediaCallback(.ended)
        await Task.yield()
        XCTAssertEqual(controller.engineState, .paused)
        XCTAssertEqual(controller.elapsed, 0)
        XCTAssertNil(controller.errorMessage)
        XCTAssertEqual(resolver.resolvedTrackIDs, ["last"])

        controller.togglePlayPause()
        system.handlers?.play()
        await controller.waitForPendingTransitionForTesting()

        XCTAssertEqual(resolver.resolvedTrackIDs, ["last", "last"])
        XCTAssertEqual(engine.loadedURLs.map(\.lastPathComponent), ["last", "last"])
        XCTAssertEqual(engine.autoplayValues, [true, true])

        oldMediaCallback(.state(.buffering))
        oldMediaCallback(.time(elapsed: 99, duration: 999))
        oldMediaCallback(.failed(message: "captured late failure"))
        oldMediaCallback(.ended)
        await Task.yield()

        XCTAssertEqual(controller.currentTrack?.id, "last")
        XCTAssertEqual(controller.elapsed, 0)
        XCTAssertNil(controller.errorMessage)
        XCTAssertEqual(resolver.resolvedTrackIDs, ["last", "last"])
    }

    func testPendingQueueTailReplayIsSupersededByShutdown() async {
        let engine = RecordingPlaybackEngine()
        let resolver = RecordingMediaResolver()
        let controller = PlaybackController(resolver: resolver, engine: engine)
        await controller.playNow(playbackTrack("last"))
        engine.send(.ended)
        await controller.waitForPendingTransitionForTesting()

        controller.play()
        controller.shutdown()
        await Task.yield()

        XCTAssertEqual(resolver.resolvedTrackIDs, ["last"])
        XCTAssertTrue(controller.queueEntries.isEmpty)
    }

    func testPendingQueueTailReplayCannotReplaceExplicitNewPlayback() async {
        let engine = RecordingPlaybackEngine()
        let resolver = RecordingMediaResolver()
        let controller = PlaybackController(resolver: resolver, engine: engine)
        await controller.playNow(playbackTrack("last"))
        engine.send(.ended)
        await controller.waitForPendingTransitionForTesting()

        controller.play()
        await controller.playNow(playbackTrack("new"))
        await Task.yield()

        XCTAssertEqual(resolver.resolvedTrackIDs, ["last", "new"])
        XCTAssertEqual(controller.currentTrack?.id, "new")
        XCTAssertEqual(engine.loadedURLs.map(\.lastPathComponent), ["last", "new"])
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

    func testNaturalEndWithoutHealthyCandidatePreservesCurrentAfterMovingFailedHistory() async {
        let engine = RecordingPlaybackEngine()
        let resolver = RecordingMediaResolver()
        let controller = PlaybackController(resolver: resolver, engine: engine)
        await controller.play(tracks: [playbackTrack("a"), playbackTrack("b")], startingAt: 0)

        engine.send(.failed(message: "a1"))
        await controller.waitForPendingTransitionForTesting()
        engine.send(.failed(message: "a2"))
        await controller.waitForPendingTransitionForTesting()
        controller.moveQueueEntry(from: 0, to: 1)

        engine.send(.ended)
        await controller.waitForPendingTransitionForTesting()

        XCTAssertEqual(controller.queueEntries.map(\.track.id), ["b", "a"])
        XCTAssertEqual(controller.currentTrack?.id, "b")
        await controller.previous()
        XCTAssertEqual(resolver.resolvedTrackIDs, ["a", "a", "b"])

        await controller.playQueueEntry(id: controller.queueEntries[0].id)
        XCTAssertEqual(controller.currentTrack?.id, "b")
        XCTAssertEqual(resolver.resolvedTrackIDs, ["a", "a", "b", "b"])
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

    func testRemovingCurrentDoesNotAutomaticallyRetryFailedReplacement() async {
        let engine = RecordingPlaybackEngine()
        let resolver = RecordingMediaResolver()
        let controller = PlaybackController(resolver: resolver, engine: engine)
        await controller.play(tracks: [playbackTrack("a"), playbackTrack("b")], startingAt: 0)

        engine.send(.failed(message: "a1"))
        await controller.waitForPendingTransitionForTesting()
        engine.send(.failed(message: "a2"))
        await controller.waitForPendingTransitionForTesting()
        controller.removeFromQueue(id: controller.queueEntries[1].id)
        await controller.waitForPendingTransitionForTesting()

        XCTAssertEqual(controller.queueEntries.map(\.track.id), ["a"])
        XCTAssertEqual(controller.currentTrack?.id, "a")
        XCTAssertEqual(resolver.resolvedTrackIDs, ["a", "a", "b"])
        XCTAssertEqual(engine.loadedURLs.map(\.lastPathComponent), ["a", "a", "b"])
        XCTAssertEqual(engine.stopCalls, 1)

        await controller.playQueueEntry(id: controller.queueEntries[0].id)
        engine.send(.failed(message: "manual retry refresh"))
        await controller.waitForPendingTransitionForTesting()

        XCTAssertEqual(controller.currentTrack?.id, "a")
        XCTAssertEqual(resolver.resolvedTrackIDs, ["a", "a", "b", "a", "a"])
        XCTAssertEqual(engine.loadedURLs.map(\.lastPathComponent), ["a", "a", "b", "a", "a"])
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
private final class FakeArtworkLoader: PlaybackArtworkLoading {
    let image: NSImage?
    private(set) var loadedURLs: [URL] = []

    init(image: NSImage?) { self.image = image }

    func load(url: URL) async -> NSImage? {
        loadedURLs.append(url)
        return image
    }
}

@MainActor
private final class RecordingSystemMediaCoordinator: SystemMediaCoordinating {
    private(set) var registerCalls = 0
    private(set) var handlers: RemotePlaybackHandlers?
    private(set) var lastTrack: Track?
    private(set) var lastElapsed: TimeInterval = 0
    private(set) var lastDuration: TimeInterval = 0
    private(set) var lastState: PlaybackEngineState = .idle
    private(set) var lastArtwork: NSImage?

    func register(_ handlers: RemotePlaybackHandlers) {
        registerCalls += 1
        self.handlers = handlers
    }
    func update(
        track: Track?, elapsed: TimeInterval, duration: TimeInterval,
        state: PlaybackEngineState, artwork: NSImage?
    ) {
        lastTrack = track
        lastElapsed = elapsed
        lastDuration = duration
        lastState = state
        lastArtwork = artwork
    }
    func clear() {
        handlers = nil
        lastTrack = nil
        lastDuration = 0
        lastArtwork = nil
    }
}

@MainActor
private final class SupersedingArtworkLoader: PlaybackArtworkLoading {
    private let currentImage: NSImage
    private let onFirstRequest: () -> Void
    private var requestCount = 0
    private var firstContinuation: CheckedContinuation<NSImage?, Never>?

    init(currentImage: NSImage, onFirstRequest: @escaping () -> Void) {
        self.currentImage = currentImage
        self.onFirstRequest = onFirstRequest
    }

    func load(url: URL) async -> NSImage? {
        requestCount += 1
        guard requestCount == 1 else { return currentImage }
        onFirstRequest()
        return await withCheckedContinuation { continuation in
            firstContinuation = continuation
        }
    }

    func resumeFirst(with image: NSImage?) {
        firstContinuation?.resume(returning: image)
        firstContinuation = nil
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
    private(set) var autoplayValues: [Bool] = []
    private(set) var playCalls = 0
    private(set) var pauseCalls = 0
    private(set) var seekValues: [TimeInterval] = []
    private(set) var volumeValues: [Float] = []
    private(set) var mutedValues: [Bool] = []
    private(set) var stopCalls = 0

    func load(url: URL, autoplay: Bool) {
        loadedURLs.append(url)
        autoplayValues.append(autoplay)
    }
    func play() { playCalls += 1 }
    func pause() { pauseCalls += 1 }
    func seek(to seconds: TimeInterval) { seekValues.append(seconds) }
    func setVolume(_ volume: Float) { volumeValues.append(volume) }
    func setMuted(_ muted: Bool) { mutedValues.append(muted) }
    func stop() { stopCalls += 1 }
    func send(_ event: PlaybackEngineEvent) { onEvent?(event) }
}

private struct PlaybackReportEvent: Equatable {
    let trackID: String
    let submission: Bool
}

@MainActor
private final class RecordingPlaybackReporter: PlaybackReporting {
    private(set) var events: [PlaybackReportEvent] = []
    private let error: Error?

    init(error: Error? = nil) {
        self.error = error
    }

    func reportPlayback(trackID: String, submission: Bool) async throws {
        events.append(.init(trackID: trackID, submission: submission))
        if let error { throw error }
    }
}

@MainActor
private final class SuspendingStartPlaybackReporter: PlaybackReporting {
    private(set) var events: [PlaybackReportEvent] = []
    private let onStart: () -> Void
    private var startContinuation: CheckedContinuation<Void, Never>?

    init(onStart: @escaping () -> Void) {
        self.onStart = onStart
    }

    func reportPlayback(trackID: String, submission: Bool) async throws {
        events.append(.init(trackID: trackID, submission: submission))
        guard !submission else { return }
        onStart()
        await withCheckedContinuation { continuation in
            startContinuation = continuation
        }
    }

    func resumeStart() {
        startContinuation?.resume()
        startContinuation = nil
    }
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
    func logout() async {}

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
