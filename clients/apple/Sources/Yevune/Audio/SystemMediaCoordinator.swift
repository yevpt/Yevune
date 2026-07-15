import AppKit
import MediaPlayer
import YevuneCoreFFI

struct RemotePlaybackHandlers {
    let play: () -> Void
    let pause: () -> Void
    let previous: () -> Void
    let next: () -> Void
    let seek: (TimeInterval) -> Void
}

@MainActor
protocol RemoteCommandSurface: AnyObject {
    func install(_ handlers: RemotePlaybackHandlers)
    func removeAll()
}

@MainActor
protocol NowPlayingSurface: AnyObject {
    var info: [String: Any]? { get set }
}

@MainActor
protocol SystemMediaCoordinating: AnyObject {
    func register(_ handlers: RemotePlaybackHandlers)
    func update(
        track: Track?, elapsed: TimeInterval, duration: TimeInterval,
        state: PlaybackEngineState, artwork: NSImage?
    )
    func clear()
}

@MainActor
final class SystemMediaCoordinator: SystemMediaCoordinating {
    private let commands: any RemoteCommandSurface
    private let nowPlaying: any NowPlayingSurface
    private var isRegistered = false

    convenience init() {
        self.init(
            commands: MPRemoteCommandSurface(),
            nowPlaying: MPNowPlayingInfoSurface()
        )
    }

    init(commands: any RemoteCommandSurface, nowPlaying: any NowPlayingSurface) {
        self.commands = commands
        self.nowPlaying = nowPlaying
    }

    func register(_ handlers: RemotePlaybackHandlers) {
        guard !isRegistered else { return }
        commands.install(handlers)
        isRegistered = true
    }

    func update(
        track: Track?, elapsed: TimeInterval, duration: TimeInterval,
        state: PlaybackEngineState, artwork: NSImage?
    ) {
        guard let track else {
            nowPlaying.info = nil
            return
        }

        var info: [String: Any] = [
            MPMediaItemPropertyTitle: track.title,
            MPMediaItemPropertyPlaybackDuration: duration,
            MPNowPlayingInfoPropertyElapsedPlaybackTime: elapsed,
            MPNowPlayingInfoPropertyPlaybackRate: state == .playing ? 1.0 : 0.0,
        ]
        if let artist = track.artist { info[MPMediaItemPropertyArtist] = artist }
        if let album = track.album { info[MPMediaItemPropertyAlbumTitle] = album }
        if let artwork {
            info[MPMediaItemPropertyArtwork] = MPMediaItemArtwork(boundsSize: artwork.size) { _ in
                artwork
            }
        }
        nowPlaying.info = info
    }

    func clear() {
        commands.removeAll()
        nowPlaying.info = nil
        isRegistered = false
    }
}

@MainActor
private final class MPRemoteCommandSurface: RemoteCommandSurface {
    private let center: MPRemoteCommandCenter

    init(center: MPRemoteCommandCenter = .shared()) {
        self.center = center
    }

    func install(_ handlers: RemotePlaybackHandlers) {
        center.playCommand.addTarget { _ in
            Task { @MainActor in handlers.play() }
            return .success
        }
        center.pauseCommand.addTarget { _ in
            Task { @MainActor in handlers.pause() }
            return .success
        }
        center.previousTrackCommand.addTarget { _ in
            Task { @MainActor in handlers.previous() }
            return .success
        }
        center.nextTrackCommand.addTarget { _ in
            Task { @MainActor in handlers.next() }
            return .success
        }
        center.changePlaybackPositionCommand.addTarget { event in
            guard let event = event as? MPChangePlaybackPositionCommandEvent else {
                return .commandFailed
            }
            let position = event.positionTime
            Task { @MainActor in handlers.seek(position) }
            return .success
        }
    }

    func removeAll() {
        center.playCommand.removeTarget(nil)
        center.pauseCommand.removeTarget(nil)
        center.previousTrackCommand.removeTarget(nil)
        center.nextTrackCommand.removeTarget(nil)
        center.changePlaybackPositionCommand.removeTarget(nil)
    }
}

@MainActor
private final class MPNowPlayingInfoSurface: NowPlayingSurface {
    private let center: MPNowPlayingInfoCenter

    init(center: MPNowPlayingInfoCenter = .default()) {
        self.center = center
    }

    var info: [String: Any]? {
        get { center.nowPlayingInfo }
        set { center.nowPlayingInfo = newValue }
    }
}

@MainActor
final class NoopSystemMediaCoordinator: SystemMediaCoordinating {
    func register(_ handlers: RemotePlaybackHandlers) {}
    func update(
        track: Track?, elapsed: TimeInterval, duration: TimeInterval,
        state: PlaybackEngineState, artwork: NSImage?
    ) {}
    func clear() {}
}
