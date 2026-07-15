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

        player.timeControlStatus = .waitingToPlayAtSpecifiedRate
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
        engine.pause()
        player.timeControlStatus = .playing
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
    private(set) var removeTimeObserverCalls = 0
    private var periodicBlock: (@Sendable (CMTime) -> Void)?

    var loadedURL: URL? { (currentItem?.asset as? AVURLAsset)?.url }
    var currentItemDuration: CMTime { duration }

    func replaceCurrentItem(with item: AVPlayerItem?) { currentItem = item }
    func play() { playCalls += 1 }
    func pause() { pauseCalls += 1 }
    func seek(to time: CMTime) { lastSeek = time.seconds }

    func addPeriodicTimeObserver(
        forInterval interval: CMTime,
        queue: DispatchQueue?,
        using block: @escaping @Sendable (CMTime) -> Void
    ) -> Any {
        periodicBlock = block
        return NSObject()
    }

    func removeTimeObserver(_ observer: Any) {
        removeTimeObserverCalls += 1
        periodicBlock = nil
    }

    func sendPeriodicTime(_ time: CMTime) {
        periodicBlock?(time)
    }
}
