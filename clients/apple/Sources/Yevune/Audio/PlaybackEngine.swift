import Foundation

enum PlaybackEngineState: Equatable {
    case idle
    case paused
    case playing
    case buffering
}

enum PlaybackEngineEvent: Equatable {
    case state(PlaybackEngineState)
    case time(elapsed: TimeInterval, duration: TimeInterval)
    case ended
    case failed(message: String)
}

@MainActor
protocol PlaybackEngine: AnyObject {
    var onEvent: ((PlaybackEngineEvent) -> Void)? { get set }

    func load(url: URL, autoplay: Bool)
    func play()
    func pause()
    func seek(to seconds: TimeInterval)
    func setVolume(_ volume: Float)
    func setMuted(_ muted: Bool)
    func stop()
}
