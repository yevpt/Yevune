import AVFoundation
import XCTest
@testable import Yevune

@MainActor
final class PlaybackEngineTests: XCTestCase {
    func testLoadReplacesItemAndHonorsAutoplay() {
        let player = FakeQueuePlayerSurface()
        let engine = AVQueuePlaybackEngine(player: player)

        engine.load(url: URL(string: "https://example.invalid/song")!, autoplay: true)

        XCTAssertEqual(player.loadedURL?.absoluteString, "https://example.invalid/song")
        XCTAssertEqual(player.playCalls, 1)
    }

    func testStopRemovesTimeObserverAndCurrentItem() {
        let player = FakeQueuePlayerSurface()
        let engine = AVQueuePlaybackEngine(player: player)
        engine.load(url: URL(string: "https://example.invalid/song")!, autoplay: false)

        engine.stop()

        XCTAssertEqual(player.removeTimeObserverCalls, 1)
        XCTAssertNil(player.loadedURL)
    }

    func testSeekAndVolumeClampAtEngineBoundary() {
        let player = FakeQueuePlayerSurface()
        let engine = AVQueuePlaybackEngine(player: player)

        engine.seek(to: -4)
        engine.setVolume(1.5)

        XCTAssertEqual(player.lastSeek, 0)
        XCTAssertEqual(player.volume, 1)
    }

    func testItemNotificationsBecomeEndedBufferingAndFailedEvents() {
        let center = NotificationCenter()
        let player = FakeQueuePlayerSurface()
        let engine = AVQueuePlaybackEngine(player: player, notificationCenter: center)
        var events: [PlaybackEngineEvent] = []
        engine.onEvent = { events.append($0) }
        engine.load(url: URL(string: "https://example.invalid/song")!, autoplay: false)
        let item = try! XCTUnwrap(player.currentItem)

        center.post(name: .AVPlayerItemPlaybackStalled, object: item)
        center.post(name: .AVPlayerItemDidPlayToEndTime, object: item)
        center.post(
            name: .AVPlayerItemFailedToPlayToEndTime,
            object: item,
            userInfo: [AVPlayerItemFailedToPlayToEndTimeErrorKey: CocoaError(.fileReadUnknown)]
        )

        XCTAssertEqual(events[0], .state(.buffering))
        XCTAssertEqual(events[1], .ended)
        guard case .failed = events[2] else { return XCTFail("expected failed event") }
    }

    func testPeriodicObserverMapsPlayerStateAndClampsInvalidTimes() {
        let player = FakeQueuePlayerSurface()
        let engine = AVQueuePlaybackEngine(player: player)
        var events: [PlaybackEngineEvent] = []
        engine.onEvent = { events.append($0) }
        engine.load(url: URL(string: "https://example.invalid/song")!, autoplay: false)

        player.sendTimeControlStatus(.waitingToPlayAtSpecifiedRate)
        player.duration = CMTime(seconds: .infinity, preferredTimescale: 600)
        player.sendPeriodicTime(CMTime(seconds: -.infinity, preferredTimescale: 600))

        XCTAssertEqual(events, [.state(.buffering), .time(elapsed: 0, duration: 0)])
    }

    func testPlayPauseAndPeriodicObserverMapPlayingAndPausedStates() {
        let player = FakeQueuePlayerSurface()
        let engine = AVQueuePlaybackEngine(player: player)
        var events: [PlaybackEngineEvent] = []
        engine.onEvent = { events.append($0) }
        engine.load(url: URL(string: "https://example.invalid/song")!, autoplay: false)

        engine.play()
        player.sendTimeControlStatus(.playing)
        engine.pause()
        player.sendTimeControlStatus(.paused)
        player.sendTimeControlStatus(.playing)
        player.duration = CMTime(seconds: 12, preferredTimescale: 600)
        player.sendPeriodicTime(CMTime(seconds: 3, preferredTimescale: 600))

        XCTAssertEqual(player.playCalls, 1)
        XCTAssertEqual(player.pauseCalls, 1)
        XCTAssertEqual(
            events,
            [.state(.playing), .state(.paused), .state(.playing), .time(elapsed: 3, duration: 12)]
        )
    }

    func testReloadAndStopRemoveObserversExactlyOnceAndIgnoreOldItem() {
        let center = NotificationCenter()
        let player = FakeQueuePlayerSurface()
        let engine = AVQueuePlaybackEngine(player: player, notificationCenter: center)
        var events: [PlaybackEngineEvent] = []
        engine.onEvent = { events.append($0) }
        engine.load(url: URL(string: "https://example.invalid/first")!, autoplay: false)
        let firstItem = try! XCTUnwrap(player.currentItem)

        engine.load(url: URL(string: "https://example.invalid/second")!, autoplay: false)
        center.post(name: .AVPlayerItemDidPlayToEndTime, object: firstItem)
        engine.stop()
        engine.stop()

        XCTAssertEqual(player.removeTimeObserverCalls, 2)
        XCTAssertEqual(events, [.state(.idle), .state(.idle)])
    }

    func testMutePassesThroughToPlayer() {
        let player = FakeQueuePlayerSurface()
        let engine = AVQueuePlaybackEngine(player: player)

        engine.setMuted(true)

        XCTAssertTrue(player.isMuted)
    }

    func testTimeControlStatusChangesPublishWithoutPeriodicTimeCallback() {
        let player = FakeQueuePlayerSurface()
        let engine = AVQueuePlaybackEngine(player: player)
        var events: [PlaybackEngineEvent] = []
        engine.onEvent = { events.append($0) }
        engine.load(url: URL(string: "https://example.invalid/song")!, autoplay: true)

        player.sendTimeControlStatus(.waitingToPlayAtSpecifiedRate)
        player.sendTimeControlStatus(.playing)
        player.sendTimeControlStatus(.paused)

        XCTAssertEqual(
            events,
            [.state(.buffering), .state(.playing), .state(.paused)]
        )
    }

    func testDeinitOffMainActorRemovesPeriodicAndStatusObservers() async {
        let player = FakeQueuePlayerSurface()
        let box = SendableBox<AVQueuePlaybackEngine>()
        box.value = AVQueuePlaybackEngine(player: player)
        box.value?.load(url: URL(string: "https://example.invalid/song")!, autoplay: false)

        await Task.detached {
            box.value = nil
        }.value

        XCTAssertEqual(player.removeTimeObserverCalls, 1)
        XCTAssertEqual(player.removeStatusObserverCalls, 1)
    }
}

private final class SendableBox<Value>: @unchecked Sendable {
    var value: Value?
}

@MainActor
private final class FakeQueuePlayerSurface: QueuePlayerSurface {
    var volume: Float = 1
    var isMuted = false
    var timeControlStatus: AVPlayer.TimeControlStatus = .paused
    var duration: CMTime = .invalid
    private(set) var currentItem: AVPlayerItem?
    private(set) var playCalls = 0
    private(set) var pauseCalls = 0
    private(set) var lastSeek: TimeInterval?
    private let periodicObservation = FakePeriodicObservation()
    private let statusObservation = FakeStatusObservation()

    var removeTimeObserverCalls: Int { periodicObservation.removeCalls }
    var removeStatusObserverCalls: Int { statusObservation.removeCalls }

    var loadedURL: URL? { (currentItem?.asset as? AVURLAsset)?.url }
    var currentItemDuration: CMTime { duration }

    func replaceCurrentItem(with item: AVPlayerItem?) { currentItem = item }
    func play() { playCalls += 1 }
    func pause() { pauseCalls += 1 }
    func seek(to time: CMTime) { lastSeek = time.seconds }

    func observePeriodicTime(
        forInterval interval: CMTime,
        using block: @escaping @MainActor @Sendable (CMTime) -> Void
    ) -> PlayerObservation {
        periodicObservation.install(block)
    }

    func observeTimeControlStatus(
        using block: @escaping @MainActor @Sendable (AVPlayer.TimeControlStatus) -> Void
    ) -> PlayerObservation {
        statusObservation.install(block)
    }

    func sendPeriodicTime(_ time: CMTime) {
        periodicObservation.send(time)
    }

    func sendTimeControlStatus(_ status: AVPlayer.TimeControlStatus) {
        timeControlStatus = status
        statusObservation.send(status)
    }
}

private final class FakePeriodicObservation: @unchecked Sendable {
    private let lock = NSLock()
    private var block: (@MainActor @Sendable (CMTime) -> Void)?
    private var removals = 0

    var removeCalls: Int {
        lock.withLock { removals }
    }

    @MainActor
    func install(_ block: @escaping @MainActor @Sendable (CMTime) -> Void) -> PlayerObservation {
        lock.withLock { self.block = block }
        return PlayerObservation { [self] in remove() }
    }

    @MainActor
    func send(_ time: CMTime) {
        let callback: (@MainActor @Sendable (CMTime) -> Void)? = lock.withLock { self.block }
        callback?(time)
    }

    private func remove() {
        lock.withLock {
            block = nil
            removals += 1
        }
    }
}

private final class FakeStatusObservation: @unchecked Sendable {
    private let lock = NSLock()
    private var block: (@MainActor @Sendable (AVPlayer.TimeControlStatus) -> Void)?
    private var removals = 0

    var removeCalls: Int {
        lock.withLock { removals }
    }

    @MainActor
    func install(
        _ block: @escaping @MainActor @Sendable (AVPlayer.TimeControlStatus) -> Void
    ) -> PlayerObservation {
        lock.withLock { self.block = block }
        return PlayerObservation { [self] in remove() }
    }

    @MainActor
    func send(_ status: AVPlayer.TimeControlStatus) {
        let callback: (@MainActor @Sendable (AVPlayer.TimeControlStatus) -> Void)? = lock.withLock { self.block }
        callback?(status)
    }

    private func remove() {
        lock.withLock {
            block = nil
            removals += 1
        }
    }
}
