import Foundation
import YevuneCoreFFI

@MainActor
final class LyricsViewModel: ObservableObject {
    @Published private(set) var state: LyricsState = .unavailable

    private let client: any MusicClientProviding
    private var generation = 0
    private var timedLyrics: TimedLyrics?

    init(client: any MusicClientProviding) {
        self.client = client
    }

    func load(trackID: String?) async {
        generation += 1
        let requestedGeneration = generation
        timedLyrics = nil

        guard let trackID else {
            state = .unavailable
            return
        }

        state = .loading
        do {
            let candidates = try await client.getLyricsBySongID(trackID)
            guard requestedGeneration == generation, !Task.isCancelled else { return }
            apply(candidates)
        } catch {
            guard requestedGeneration == generation, !Task.isCancelled else { return }
            state = .failed("歌词加载失败")
        }
    }

    func update(elapsed: TimeInterval) {
        guard let timedLyrics, elapsed.isFinite else { return }
        let adjustedMilliseconds = elapsed * 1_000 - Double(timedLyrics.offset)
        let currentLine = timedLyrics.lines.lastIndex {
            Double($0.start) <= adjustedMilliseconds
        } ?? -1
        let nextState = LyricsState.synced(
            lines: timedLyrics.lines.map(\.value),
            currentLine: currentLine
        )
        if state != nextState { state = nextState }
    }

    private func apply(_ candidates: [StructuredLyrics]) {
        if let synchronized = candidates.first(where: {
            $0.synced && $0.lines.contains(where: { $0.start != nil })
        }) {
            let lines = synchronized.lines.compactMap { line -> TimedLine? in
                guard let start = line.start else { return nil }
                return TimedLine(start: start, value: line.value)
            }.sorted { $0.start < $1.start }
            if !lines.isEmpty {
                timedLyrics = TimedLyrics(offset: synchronized.offset, lines: lines)
                state = .synced(lines: lines.map(\.value), currentLine: -1)
                return
            }
        }

        if let plain = candidates.first(where: { !$0.lines.isEmpty }) {
            state = .unsynced(plain.lines.map(\.value).joined(separator: "\n"))
        } else {
            state = .unavailable
        }
    }
}

private struct TimedLyrics {
    let offset: Int64
    let lines: [TimedLine]
}

private struct TimedLine {
    let start: UInt64
    let value: String
}
