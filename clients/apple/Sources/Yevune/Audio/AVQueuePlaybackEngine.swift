import AVFoundation

@MainActor
protocol QueuePlayerSurface: AnyObject {
    var volume: Float { get set }
    var isMuted: Bool { get set }
    var currentItem: AVPlayerItem? { get }
    var currentItemDuration: CMTime { get }
    var timeControlStatus: AVPlayer.TimeControlStatus { get }

    func replaceCurrentItem(with item: AVPlayerItem?)
    func play()
    func pause()
    func seek(to time: CMTime)
    func addPeriodicTimeObserver(
        forInterval interval: CMTime,
        queue: DispatchQueue?,
        using block: @escaping @Sendable (CMTime) -> Void
    ) -> Any
    func removeTimeObserver(_ observer: Any)
}

extension AVQueuePlayer: QueuePlayerSurface {
    var currentItemDuration: CMTime { currentItem?.duration ?? .invalid }
}

@MainActor
final class AVQueuePlaybackEngine: PlaybackEngine {
    var onEvent: ((PlaybackEngineEvent) -> Void)?

    private let player: any QueuePlayerSurface
    private let notificationCenter: NotificationCenter
    private var timeObserver: Any?
    private var itemObservers: [NSObjectProtocol] = []

    init(
        player: any QueuePlayerSurface = AVQueuePlayer(),
        notificationCenter: NotificationCenter = .default
    ) {
        self.player = player
        self.notificationCenter = notificationCenter
    }

    deinit {
        MainActor.assumeIsolated {
            removeObservers()
        }
    }

    func load(url: URL, autoplay: Bool) {
        removeObservers()
        player.replaceCurrentItem(with: AVPlayerItem(url: url))
        installObservers()
        if autoplay {
            play()
        }
    }

    func play() {
        player.play()
        onEvent?(.state(.playing))
    }

    func pause() {
        player.pause()
        onEvent?(.state(.paused))
    }

    func seek(to seconds: TimeInterval) {
        player.seek(to: CMTime(seconds: finiteNonnegative(seconds), preferredTimescale: 600))
    }

    func setVolume(_ volume: Float) {
        player.volume = min(max(volume.isFinite ? volume : 0, 0), 1)
    }

    func setMuted(_ muted: Bool) {
        player.isMuted = muted
    }

    func stop() {
        player.pause()
        removeObservers()
        player.replaceCurrentItem(with: nil)
        onEvent?(.state(.idle))
    }

    private func installObservers() {
        guard let item = player.currentItem else { return }

        timeObserver = player.addPeriodicTimeObserver(
            forInterval: CMTime(seconds: 0.5, preferredTimescale: 600),
            queue: .main
        ) { [weak self] time in
            MainActor.assumeIsolated {
                guard let self else { return }
                self.onEvent?(.state(self.playbackState))
                self.onEvent?(
                    .time(
                        elapsed: self.finiteNonnegative(time.seconds),
                        duration: self.finiteNonnegative(self.player.currentItemDuration.seconds)
                    )
                )
            }
        }

        itemObservers = [
            observe(.AVPlayerItemPlaybackStalled, item: item) { engine, _ in
                engine.onEvent?(.state(.buffering))
            },
            observe(.AVPlayerItemDidPlayToEndTime, item: item) { engine, _ in
                engine.onEvent?(.ended)
            },
            observe(.AVPlayerItemFailedToPlayToEndTime, item: item) { engine, notification in
                let error = notification.userInfo?[AVPlayerItemFailedToPlayToEndTimeErrorKey] as? Error
                engine.onEvent?(.failed(message: error?.localizedDescription ?? "Playback failed"))
            },
        ]
    }

    private func observe(
        _ name: Notification.Name,
        item: AVPlayerItem,
        handler: @escaping @MainActor (AVQueuePlaybackEngine, Notification) -> Void
    ) -> NSObjectProtocol {
        notificationCenter.addObserver(forName: name, object: item, queue: .main) { [weak self] notification in
            MainActor.assumeIsolated {
                guard let self else { return }
                handler(self, notification)
            }
        }
    }

    private func removeObservers() {
        itemObservers.forEach(notificationCenter.removeObserver)
        itemObservers.removeAll()
        if let timeObserver {
            player.removeTimeObserver(timeObserver)
            self.timeObserver = nil
        }
    }

    private var playbackState: PlaybackEngineState {
        switch player.timeControlStatus {
        case .paused:
            .paused
        case .playing:
            .playing
        case .waitingToPlayAtSpecifiedRate:
            .buffering
        @unknown default:
            .paused
        }
    }

    private func finiteNonnegative<T: BinaryFloatingPoint>(_ value: T) -> T {
        value.isFinite ? max(value, 0) : 0
    }
}
