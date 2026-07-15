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
    func observePeriodicTime(
        forInterval interval: CMTime,
        using block: @escaping @MainActor @Sendable (CMTime) -> Void
    ) -> PlayerObservation
    func observeTimeControlStatus(
        using block: @escaping @MainActor @Sendable (AVPlayer.TimeControlStatus) -> Void
    ) -> PlayerObservation
}

extension AVQueuePlayer: QueuePlayerSurface {
    var currentItemDuration: CMTime { currentItem?.duration ?? .invalid }

    func observePeriodicTime(
        forInterval interval: CMTime,
        using block: @escaping @MainActor @Sendable (CMTime) -> Void
    ) -> PlayerObservation {
        let token = addPeriodicTimeObserver(forInterval: interval, queue: .main) { time in
            Task { @MainActor in
                block(time)
            }
        }
        return PlayerObservation { [self] in
            removeTimeObserver(token)
        }
    }

    func observeTimeControlStatus(
        using block: @escaping @MainActor @Sendable (AVPlayer.TimeControlStatus) -> Void
    ) -> PlayerObservation {
        let observation = observe(\.timeControlStatus, options: [.new]) { player, _ in
            let status = player.timeControlStatus
            Task { @MainActor in
                block(status)
            }
        }
        return PlayerObservation {
            observation.invalidate()
        }
    }
}

final class PlayerObservation: @unchecked Sendable {
    private let lock = NSLock()
    private var cancellation: (() -> Void)?

    init(cancellation: @escaping () -> Void) {
        self.cancellation = cancellation
    }

    func cancel() {
        lock.lock()
        let cancellation = cancellation
        self.cancellation = nil
        lock.unlock()
        cancellation?()
    }

    deinit {
        cancel()
    }
}

private final class PlayerObservationStore: @unchecked Sendable {
    private let lock = NSLock()
    private var observations: [PlayerObservation] = []

    func insert(_ observation: PlayerObservation) {
        lock.lock()
        observations.append(observation)
        lock.unlock()
    }

    func removeAll() {
        lock.lock()
        let observations = observations
        self.observations.removeAll()
        lock.unlock()
        observations.forEach { $0.cancel() }
    }

    deinit {
        removeAll()
    }
}

@MainActor
final class AVQueuePlaybackEngine: PlaybackEngine {
    var onEvent: ((PlaybackEngineEvent) -> Void)?

    private let player: any QueuePlayerSurface
    private let notificationCenter: NotificationCenter
    private nonisolated let observations = PlayerObservationStore()

    init(
        player: any QueuePlayerSurface = AVQueuePlayer(),
        notificationCenter: NotificationCenter = .default
    ) {
        self.player = player
        self.notificationCenter = notificationCenter
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
    }

    func pause() {
        player.pause()
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

        observations.insert(player.observePeriodicTime(
            forInterval: CMTime(seconds: 0.5, preferredTimescale: 600)
        ) { [weak self] time in
            guard let self else { return }
            self.onEvent?(
                .time(
                    elapsed: self.finiteNonnegative(time.seconds),
                    duration: self.finiteNonnegative(self.player.currentItemDuration.seconds)
                )
            )
        })

        observations.insert(player.observeTimeControlStatus { [weak self] status in
            self?.onEvent?(.state(Self.playbackState(for: status)))
        })

        [
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
        ].forEach(observations.insert)
    }

    private func observe(
        _ name: Notification.Name,
        item: AVPlayerItem,
        handler: @escaping @MainActor (AVQueuePlaybackEngine, Notification) -> Void
    ) -> PlayerObservation {
        let token = notificationCenter.addObserver(forName: name, object: item, queue: .main) { [weak self] notification in
            MainActor.assumeIsolated {
                guard let self else { return }
                handler(self, notification)
            }
        }
        return PlayerObservation { [notificationCenter] in
            notificationCenter.removeObserver(token)
        }
    }

    private func removeObservers() {
        observations.removeAll()
    }

    private static func playbackState(for status: AVPlayer.TimeControlStatus) -> PlaybackEngineState {
        switch status {
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
