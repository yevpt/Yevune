import Foundation
import YevuneCoreFFI

enum TrackBatchAction: Equatable {
    case update(TagUpdate)
    case delete
}

enum TrackBatchItemState: Equatable {
    case pending
    case succeeded
    case skipped
    case failed(String)
}

struct TrackBatchItemResult: Identifiable, Equatable {
    let track: Track
    var id: String { track.id }
    var state: TrackBatchItemState
}

@MainActor
final class TrackBatchOperationController: ObservableObject {
    @Published private(set) var albumID: String?
    @Published private(set) var action: TrackBatchAction?
    @Published private(set) var results: [TrackBatchItemResult] = []
    @Published private(set) var currentTrackID: String?
    @Published private(set) var isRunning = false
    @Published private(set) var stopRequested = false

    var totalCount: Int { results.count }
    var completedCount: Int {
        results.count { result in
            switch result.state {
            case .succeeded, .failed:
                true
            case .pending, .skipped:
                false
            }
        }
    }
    var succeededCount: Int { results.count { $0.state == .succeeded } }
    var failedCount: Int {
        results.count {
            if case .failed = $0.state { return true }
            return false
        }
    }
    var skippedCount: Int { results.count { $0.state == .skipped } }

    private let client: any MusicClientProviding
    private var generation = 0

    init(client: any MusicClientProviding) {
        self.client = client
    }

    func run(
        tracks: [Track],
        action: TrackBatchAction,
        onFinished: @escaping @MainActor () async -> Void
    ) async {
        await performRun(tracks: tracks, action: action, onFinished: onFinished)
    }

    func stop() {
        guard isRunning else { return }
        stopRequested = true
    }

    func retryFailed(onFinished: @escaping @MainActor () async -> Void) async {
        guard let action else { return }
        let failedTracks = results.compactMap { result -> Track? in
            if case .failed = result.state { return result.track }
            return nil
        }
        guard !failedTracks.isEmpty else { return }
        await performRun(tracks: failedTracks, action: action, onFinished: onFinished)
    }

    func reset(for albumID: String) {
        generation += 1
        self.albumID = albumID
        if isRunning {
            stopRequested = true
        } else {
            clearRunState()
        }
    }

    private func performRun(
        tracks: [Track],
        action: TrackBatchAction,
        onFinished: @escaping @MainActor () async -> Void
    ) async {
        guard !isRunning, !tracks.isEmpty else { return }

        let runGeneration = generation
        let runAlbumID = albumID
        self.action = action
        results = tracks.map { TrackBatchItemResult(track: $0, state: .pending) }
        currentTrackID = nil
        isRunning = true
        stopRequested = false

        for (index, track) in tracks.enumerated() {
            if stopRequested {
                markRemainingSkipped(from: index, generation: runGeneration, albumID: runAlbumID)
                break
            }

            if isCurrent(runGeneration, albumID: runAlbumID) {
                currentTrackID = track.id
            }
            do {
                try await perform(action, trackID: track.id)
                publish(.succeeded, at: index, generation: runGeneration, albumID: runAlbumID)
            } catch {
                publish(
                    .failed(LibraryOperationErrorPresentation.message(error)),
                    at: index,
                    generation: runGeneration,
                    albumID: runAlbumID
                )
            }
        }

        if isCurrent(runGeneration, albumID: runAlbumID) {
            currentTrackID = nil
        } else {
            clearRunState()
        }

        await onFinished()

        if !isCurrent(runGeneration, albumID: runAlbumID) {
            clearRunState()
        }
        currentTrackID = nil
        isRunning = false
    }

    private func perform(_ action: TrackBatchAction, trackID: String) async throws {
        switch action {
        case let .update(update):
            try await client.updateTags(id: trackID, update: update)
        case .delete:
            try await client.deleteTrack(id: trackID)
        }
    }

    private func publish(
        _ state: TrackBatchItemState,
        at index: Int,
        generation runGeneration: Int,
        albumID runAlbumID: String?
    ) {
        guard isCurrent(runGeneration, albumID: runAlbumID) else { return }
        results[index].state = state
    }

    private func markRemainingSkipped(from index: Int, generation runGeneration: Int, albumID runAlbumID: String?) {
        guard isCurrent(runGeneration, albumID: runAlbumID) else { return }
        for remainingIndex in index..<results.count {
            results[remainingIndex].state = .skipped
        }
    }

    private func isCurrent(_ runGeneration: Int, albumID runAlbumID: String?) -> Bool {
        runGeneration == generation && runAlbumID == albumID
    }

    private func clearRunState() {
        action = nil
        results = []
        currentTrackID = nil
        stopRequested = false
    }
}
