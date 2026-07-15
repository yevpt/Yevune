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
    func observeStatus(
        of item: AVPlayerItem,
        using block: @escaping @MainActor @Sendable (AVPlayerItem.Status) -> Void
    ) -> PlayerObservation
}

extension AVQueuePlayer: QueuePlayerSurface {
    var currentItemDuration: CMTime { currentItem?.duration ?? .invalid }

    func observePeriodicTime(
        forInterval interval: CMTime,
        using block: @escaping @MainActor @Sendable (CMTime) -> Void
    ) -> PlayerObservation {
        let observation = PlayerObservation()
        let token = addPeriodicTimeObserver(forInterval: interval, queue: .main) { time in
            observation.deliverOnMain {
                block(time)
            }
        }
        observation.setCancellation { [self] in
            removeTimeObserver(token)
        }
        return observation
    }

    func observeTimeControlStatus(
        using block: @escaping @MainActor @Sendable (AVPlayer.TimeControlStatus) -> Void
    ) -> PlayerObservation {
        let playerObservation = PlayerObservation()
        let token = observe(\.timeControlStatus, options: [.new]) { player, _ in
            let status = player.timeControlStatus
            playerObservation.deliverOnMain {
                block(status)
            }
        }
        playerObservation.setCancellation {
            token.invalidate()
        }
        return playerObservation
    }

    func observeStatus(
        of item: AVPlayerItem,
        using block: @escaping @MainActor @Sendable (AVPlayerItem.Status) -> Void
    ) -> PlayerObservation {
        let itemObservation = PlayerObservation()
        let token = item.observe(\.status, options: [.initial, .new]) { item, _ in
            let status = item.status
            itemObservation.deliverOnMain { block(status) }
        }
        itemObservation.setCancellation { token.invalidate() }
        return itemObservation
    }
}

final class PlayerObservation: @unchecked Sendable {
    private let lock = NSLock()
    private var isActive = true
    private var cancellation: (() -> Void)?

    init(cancellation: (() -> Void)? = nil) {
        self.cancellation = cancellation
    }

    func setCancellation(_ cancellation: @escaping () -> Void) {
        lock.lock()
        let cancelImmediately = !isActive
        if isActive {
            precondition(self.cancellation == nil)
            self.cancellation = cancellation
        }
        lock.unlock()
        if cancelImmediately {
            cancellation()
        }
    }

    func cancel() {
        lock.lock()
        isActive = false
        let cancellation = cancellation
        self.cancellation = nil
        lock.unlock()
        cancellation?()
    }

    func deliverOnMain(_ block: @escaping @MainActor @Sendable () -> Void) {
        Task { @MainActor [self] in
            performIfActive(block)
        }
    }

    @MainActor
    func performIfActive(_ block: @MainActor () -> Void) {
        lock.lock()
        let isActive = isActive
        lock.unlock()
        if isActive {
            block()
        }
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
    private var didReportCurrentItemFailure = false

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
        didReportCurrentItemFailure = false

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

        observations.insert(player.observeStatus(of: item) { [weak self] status in
            guard status == .failed else { return }
            self?.reportCurrentItemFailure(message: item.error?.localizedDescription)
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
                engine.reportCurrentItemFailure(message: error?.localizedDescription)
            },
        ].forEach(observations.insert)
    }

    private func observe(
        _ name: Notification.Name,
        item: AVPlayerItem,
        handler: @escaping @MainActor (AVQueuePlaybackEngine, Notification) -> Void
    ) -> PlayerObservation {
        let observation = PlayerObservation()
        let token = notificationCenter.addObserver(forName: name, object: item, queue: .main) { [weak self] notification in
            MainActor.assumeIsolated {
                observation.performIfActive {
                    guard let self else { return }
                    handler(self, notification)
                }
            }
        }
        observation.setCancellation { [notificationCenter] in
            notificationCenter.removeObserver(token)
        }
        return observation
    }

    private func removeObservers() {
        observations.removeAll()
        didReportCurrentItemFailure = false
    }

    private func reportCurrentItemFailure(message: String?) {
        guard !didReportCurrentItemFailure else { return }
        didReportCurrentItemFailure = true
        onEvent?(.failed(message: message ?? "Playback failed"))
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
