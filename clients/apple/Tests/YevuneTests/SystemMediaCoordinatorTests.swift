import AppKit
import MediaPlayer
import XCTest
@testable import Yevune

@MainActor
final class SystemMediaCoordinatorTests: XCTestCase {
    func testRemoteCommandsInvokeSingleControllerHandlers() {
        let commands = FakeRemoteCommandSurface()
        let nowPlaying = FakeNowPlayingSurface()
        let coordinator = SystemMediaCoordinator(commands: commands, nowPlaying: nowPlaying)
        var calls: [String] = []

        coordinator.register(.init(
            play: { calls.append("play") }, pause: { calls.append("pause") },
            previous: { calls.append("previous") }, next: { calls.append("next") },
            seek: { calls.append("seek:\($0)") }
        ))
        commands.sendPlay()
        commands.sendPause()
        commands.sendPrevious()
        commands.sendNext()
        commands.sendSeek(42)

        XCTAssertEqual(calls, ["play", "pause", "previous", "next", "seek:42.0"])
    }

    func testRegisterInstallsTargetsExactlyOnceUntilClear() {
        let commands = FakeRemoteCommandSurface()
        let coordinator = SystemMediaCoordinator(
            commands: commands, nowPlaying: FakeNowPlayingSurface()
        )
        let handlers = RemotePlaybackHandlers(
            play: {}, pause: {}, previous: {}, next: {}, seek: { _ in }
        )

        coordinator.register(handlers)
        coordinator.register(handlers)
        XCTAssertEqual(commands.installCalls, 1)

        coordinator.clear()
        coordinator.register(handlers)
        XCTAssertEqual(commands.installCalls, 2)
    }

    func testUpdatePublishesTrackTimingAndRate() {
        let nowPlaying = FakeNowPlayingSurface()
        let coordinator = SystemMediaCoordinator(
            commands: FakeRemoteCommandSurface(), nowPlaying: nowPlaying
        )

        coordinator.update(
            track: playbackTrack("1", title: "Song", duration: 180),
            elapsed: 42, duration: 180, state: .playing, artwork: nil
        )

        XCTAssertEqual(nowPlaying.info?[MPMediaItemPropertyTitle] as? String, "Song")
        XCTAssertEqual(nowPlaying.info?[MPMediaItemPropertyArtist] as? String, "Artist")
        XCTAssertEqual(nowPlaying.info?[MPMediaItemPropertyAlbumTitle] as? String, "Album")
        XCTAssertEqual(nowPlaying.info?[MPMediaItemPropertyPlaybackDuration] as? Double, 180)
        XCTAssertEqual(nowPlaying.info?[MPNowPlayingInfoPropertyElapsedPlaybackTime] as? Double, 42)
        XCTAssertEqual(nowPlaying.info?[MPNowPlayingInfoPropertyPlaybackRate] as? Double, 1)
    }

    func testUpdatePublishesArtwork() {
        let nowPlaying = FakeNowPlayingSurface()
        let coordinator = SystemMediaCoordinator(
            commands: FakeRemoteCommandSurface(), nowPlaying: nowPlaying
        )

        coordinator.update(
            track: playbackTrack("1"), elapsed: 0, duration: 180,
            state: .paused, artwork: NSImage(size: NSSize(width: 10, height: 10))
        )

        XCTAssertNotNil(nowPlaying.info?[MPMediaItemPropertyArtwork] as? MPMediaItemArtwork)
        XCTAssertEqual(nowPlaying.info?[MPNowPlayingInfoPropertyPlaybackRate] as? Double, 0)
    }

    func testClearRemovesTargetsAndNowPlayingInfo() {
        let commands = FakeRemoteCommandSurface()
        let nowPlaying = FakeNowPlayingSurface()
        let coordinator = SystemMediaCoordinator(commands: commands, nowPlaying: nowPlaying)
        coordinator.register(.init(
            play: {}, pause: {}, previous: {}, next: {}, seek: { _ in }
        ))
        nowPlaying.info = ["title": "Song"]

        coordinator.clear()

        XCTAssertEqual(commands.targetCount, 0)
        XCTAssertNil(nowPlaying.info)
    }
}

@MainActor
private final class FakeRemoteCommandSurface: RemoteCommandSurface {
    var handlers: RemotePlaybackHandlers?
    private(set) var installCalls = 0
    var targetCount: Int { handlers == nil ? 0 : 5 }

    func install(_ handlers: RemotePlaybackHandlers) {
        installCalls += 1
        self.handlers = handlers
    }
    func removeAll() { handlers = nil }
    func sendPlay() { handlers?.play() }
    func sendPause() { handlers?.pause() }
    func sendPrevious() { handlers?.previous() }
    func sendNext() { handlers?.next() }
    func sendSeek(_ seconds: TimeInterval) { handlers?.seek(seconds) }
}

@MainActor
private final class FakeNowPlayingSurface: NowPlayingSurface {
    var info: [String: Any]?
}
