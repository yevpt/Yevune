import Foundation
import YevuneCoreFFI

@MainActor
final class PlaybackController: ObservableObject {
    @Published private(set) var queueEntries: [QueueEntry] = []
    @Published private(set) var currentTrack: Track?
    @Published private(set) var coverURL: URL?
    @Published private(set) var engineState: PlaybackEngineState = .idle
    @Published private(set) var elapsed: TimeInterval = 0
    @Published private(set) var duration: TimeInterval = 0
    @Published private(set) var errorMessage: String?
    @Published private(set) var isMuted = false
    @Published private(set) var volume: Float = 1

    private let resolver: any PlaybackMediaResolving
    private let engine: any PlaybackEngine
    private let shuffle: ([QueueEntry]) -> [QueueEntry]
    private var queue = PlaybackQueue()
    private var loadGeneration = 0
    private var hasActiveMediaSession = false

    init(
        resolver: any PlaybackMediaResolving,
        engine: any PlaybackEngine,
        shuffle: @escaping ([QueueEntry]) -> [QueueEntry] = { $0.shuffled() }
    ) {
        self.resolver = resolver
        self.engine = engine
        self.shuffle = shuffle
    }

    func play(tracks: [Track], startingAt index: Int) async {
        queue.replace(with: tracks, startingAt: index)
        synchronizeQueueState()
        if let entry = queue.current {
            await load(entry)
        } else {
            beginMediaTransition()
        }
    }

    func playNow(_ track: Track) async {
        await play(tracks: [track], startingAt: 0)
    }

    func playNext(_ track: Track) {
        queue.insertNext(track)
        synchronizeQueueState()
    }

    func addToQueue(_ track: Track) {
        queue.append(track)
        synchronizeQueueState()
    }

    func togglePlayPause() {
        switch engineState {
        case .playing, .buffering:
            engine.pause()
        case .idle, .paused:
            engine.play()
        }
    }

    func previous() async {
        guard let entry = queue.previous() else { return }
        synchronizeQueueState()
        await load(entry)
    }

    func next() async {
        guard let entry = queue.nextAfterManualSkip() else { return }
        synchronizeQueueState()
        await load(entry)
    }

    func seek(to seconds: TimeInterval) {
        engine.seek(to: seconds)
    }

    func setVolume(_ value: Float) {
        volume = min(max(value.isFinite ? value : 0, 0), 1)
        engine.setVolume(volume)
    }

    func toggleMuted() {
        isMuted.toggle()
        engine.setMuted(isMuted)
    }

    func shutdown() {
        beginMediaTransition(stopEngine: false)
        engine.stop()
        queue = PlaybackQueue()
        synchronizeQueueState()
    }

    private func load(_ entry: QueueEntry) async {
        let generation = beginMediaTransition()
        do {
            let media = try await resolver.resolve(track: entry.track)
            guard generation == loadGeneration else { return }
            coverURL = media.coverURL
            hasActiveMediaSession = true
            installEngineObserver()
            engine.load(url: media.streamURL, autoplay: true)
        } catch {
            guard generation == loadGeneration else { return }
            errorMessage = safePlaybackErrorMessage()
        }
    }

    private func synchronizeQueueState() {
        queueEntries = queue.entries
        currentTrack = queue.current?.track
    }

    @discardableResult
    private func beginMediaTransition(stopEngine: Bool = true) -> Int {
        loadGeneration += 1
        engine.onEvent = nil
        if hasActiveMediaSession {
            hasActiveMediaSession = false
            if stopEngine {
                engine.stop()
            }
        }
        engineState = .idle
        elapsed = 0
        duration = 0
        coverURL = nil
        errorMessage = nil
        return loadGeneration
    }

    private func installEngineObserver() {
        engine.onEvent = { [weak self] event in
            self?.handle(event)
        }
    }

    private func handle(_ event: PlaybackEngineEvent) {
        guard hasActiveMediaSession else { return }
        switch event {
        case let .state(state):
            engineState = state
        case let .time(elapsed, duration):
            self.elapsed = elapsed
            self.duration = duration
        case .ended:
            let generation = loadGeneration
            Task { @MainActor [weak self] in
                await self?.advanceAfterNaturalEnd(expectedGeneration: generation)
            }
        case .failed:
            errorMessage = safePlaybackErrorMessage()
        }
    }

    private func advanceAfterNaturalEnd(expectedGeneration: Int) async {
        guard expectedGeneration == loadGeneration else { return }
        guard let entry = queue.nextAfterNaturalEnd() else { return }
        synchronizeQueueState()
        await load(entry)
    }

    private func safePlaybackErrorMessage() -> String {
        guard let title = currentTrack?.title, !title.isEmpty else { return "播放失败" }
        return "无法播放「\(title)」"
    }
}
