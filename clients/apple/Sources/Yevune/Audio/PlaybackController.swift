import Foundation
import YevuneCoreFFI

@MainActor
final class PlaybackController: ObservableObject {
    private enum LoadOutcome {
        case loaded
        case resolutionFailed
        case superseded
    }

    @Published private(set) var queueEntries: [QueueEntry] = []
    @Published private(set) var currentTrack: Track?
    @Published private(set) var coverURL: URL?
    @Published private(set) var engineState: PlaybackEngineState = .idle
    @Published private(set) var elapsed: TimeInterval = 0
    @Published private(set) var duration: TimeInterval = 0
    @Published private(set) var errorMessage: String?
    @Published private(set) var isShuffled = false
    @Published private(set) var repeatMode: PlaybackRepeatMode = .off
    @Published private(set) var isMuted = false
    @Published private(set) var volume: Float = 1

    private let resolver: any PlaybackMediaResolving
    private let engine: any PlaybackEngine
    private let shuffle: ([QueueEntry]) -> [QueueEntry]
    private var queue = PlaybackQueue()
    private var loadGeneration = 0
    private var hasActiveMediaSession = false
    private var pendingTransition: Task<Void, Never>?
    private var retryCounts: [UUID: Int] = [:]
    private var failedInCycle: Set<UUID> = []

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
        retryCounts.removeAll()
        failedInCycle.removeAll()
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

    func removeFromQueue(id: UUID) {
        guard queue.entries.contains(where: { $0.id == id }) else { return }
        let removesCurrent = queue.current?.id == id
        let generation = removesCurrent ? beginMediaTransition(stopEngine: false) : loadGeneration
        if removesCurrent {
            engine.stop()
        }
        queue.remove(id: id)
        pruneFailureStateToCurrentQueue()
        synchronizeQueueState()
        guard removesCurrent, let entry = queue.current else { return }
        pendingTransition = Task { @MainActor [weak self] in
            guard let self,
                  generation == self.loadGeneration,
                  self.queue.current?.id == entry.id
            else { return }
            if self.failedInCycle.contains(entry.id) {
                await self.advancePastFailedEntries(
                    message: nil,
                    expectedGeneration: generation,
                    stopWhenNoCandidate: false
                )
                return
            }
            await self.load(entry, stopEngine: false)
        }
    }

    func moveQueueEntry(from source: Int, to destination: Int) {
        queue.move(from: source, to: destination)
        synchronizeQueueState()
    }

    func clearUpcoming() {
        queue.clearUpcoming()
        pruneFailureStateToCurrentQueue()
        synchronizeQueueState()
    }

    func setShuffled(_ enabled: Bool) {
        queue.setShuffled(enabled, using: shuffle)
        synchronizeQueueState()
    }

    func cycleRepeatMode() {
        switch queue.repeatMode {
        case .off: queue.repeatMode = .all
        case .all: queue.repeatMode = .one
        case .one: queue.repeatMode = .off
        }
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
        clearFailureState(for: entry.id)
        synchronizeQueueState()
        await load(entry)
    }

    func next() async {
        guard let entry = queue.nextAfterManualSkip() else { return }
        clearFailureState(for: entry.id)
        synchronizeQueueState()
        await load(entry)
    }

    func playQueueEntry(id: UUID) async {
        guard let entry = queue.select(id: id) else { return }
        clearFailureState(for: entry.id)
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
        retryCounts.removeAll()
        failedInCycle.removeAll()
        synchronizeQueueState()
    }

    func waitForPendingTransitionForTesting() async {
        let transition = pendingTransition
        await transition?.value
    }

    @discardableResult
    private func load(_ entry: QueueEntry, stopEngine: Bool = true) async -> LoadOutcome {
        let generation = beginMediaTransition(stopEngine: stopEngine)
        do {
            let media = try await resolver.resolve(track: entry.track)
            guard generation == loadGeneration else { return .superseded }
            coverURL = media.coverURL
            hasActiveMediaSession = true
            installEngineObserver()
            engine.load(url: media.streamURL, autoplay: true)
            return .loaded
        } catch {
            guard generation == loadGeneration else { return .superseded }
            errorMessage = safePlaybackErrorMessage()
            return .resolutionFailed
        }
    }

    private func synchronizeQueueState() {
        queueEntries = queue.entries
        currentTrack = queue.current?.track
        isShuffled = queue.isShuffled
        repeatMode = queue.repeatMode
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
            pendingTransition = Task { @MainActor [weak self] in
                guard let self else { return }
                await self.advanceAfterNaturalEnd(expectedGeneration: generation)
            }
        case .failed:
            guard let entry = queue.current else { return }
            let generation = loadGeneration
            pendingTransition = Task { @MainActor [weak self] in
                guard let self else { return }
                await self.recoverFromFailure(for: entry, expectedGeneration: generation)
            }
        }
    }

    private func advanceAfterNaturalEnd(expectedGeneration: Int) async {
        guard expectedGeneration == loadGeneration else { return }
        if queue.repeatMode == .one,
           let entry = queue.nextAfterNaturalEnd(),
           !failedInCycle.contains(entry.id) {
            synchronizeQueueState()
            await load(entry)
            return
        }
        await advancePastFailedEntries(
            message: nil,
            expectedGeneration: expectedGeneration,
            stopWhenNoCandidate: false
        )
    }

    private func recoverFromFailure(for entry: QueueEntry, expectedGeneration: Int) async {
        guard expectedGeneration == loadGeneration, queue.current?.id == entry.id else { return }
        if retryCounts[entry.id, default: 0] == 0 {
            retryCounts[entry.id] = 1
            switch await load(entry, stopEngine: false) {
            case .loaded, .superseded:
                return
            case .resolutionFailed:
                break
            }
        }

        failedInCycle.insert(entry.id)
        let message = "无法播放 \(entry.track.title)，已跳到下一首"
        await advancePastFailedEntries(
            message: message,
            expectedGeneration: loadGeneration,
            stopWhenNoCandidate: true
        )
    }

    private func advancePastFailedEntries(
        message: String?,
        expectedGeneration: Int,
        stopWhenNoCandidate: Bool
    ) async {
        guard expectedGeneration == loadGeneration else { return }
        if failedInCycle.count >= queue.entries.count {
            if stopWhenNoCandidate {
                stopAfterFailedCycle(message: message ?? safePlaybackErrorMessage())
            }
            return
        }

        var candidateQueue = queue
        for _ in 0..<candidateQueue.entries.count {
            guard let entry = candidateQueue.nextAfterManualSkip() else {
                if stopWhenNoCandidate {
                    stopAfterFailedCycle(message: message ?? safePlaybackErrorMessage())
                }
                return
            }
            if failedInCycle.contains(entry.id) { continue }
            queue = candidateQueue
            synchronizeQueueState()
            if case .loaded = await load(entry, stopEngine: false), let message {
                errorMessage = message
            }
            return
        }

        stopAfterFailedCycle(message: message ?? safePlaybackErrorMessage())
    }

    private func stopAfterFailedCycle(message: String) {
        beginMediaTransition(stopEngine: false)
        engine.stop()
        errorMessage = message
    }

    private func clearFailureState(for id: UUID) {
        retryCounts[id] = nil
        failedInCycle.remove(id)
    }

    private func pruneFailureStateToCurrentQueue() {
        let existingIDs = Set(queue.entries.map(\.id))
        retryCounts = retryCounts.filter { existingIDs.contains($0.key) }
        failedInCycle.formIntersection(existingIDs)
    }

    private func safePlaybackErrorMessage() -> String {
        guard let title = currentTrack?.title, !title.isEmpty else { return "播放失败" }
        return "无法播放「\(title)」"
    }
}
