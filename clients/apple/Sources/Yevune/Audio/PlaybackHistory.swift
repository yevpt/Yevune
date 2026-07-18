import Foundation

protocol PlaybackReporting: Sendable {
    func reportPlayback(trackID: String, submission: Bool) async throws
}

struct NoopPlaybackReporter: PlaybackReporting {
    func reportPlayback(trackID _: String, submission _: Bool) async throws {}
}

struct PlaybackHistorySession {
    let trackID: String
    private(set) var listened: TimeInterval = 0
    private(set) var submissionScheduled = false
    private var lastElapsed: TimeInterval?

    init(trackID: String) {
        self.trackID = trackID
    }

    mutating func observe(
        elapsed: TimeInterval,
        duration: TimeInterval,
        state: PlaybackEngineState
    ) -> Bool {
        guard elapsed.isFinite, elapsed >= 0 else { return false }
        defer { lastElapsed = elapsed }

        if state == .playing, let lastElapsed {
            let delta = elapsed - lastElapsed
            if delta > 0, delta <= 5 {
                listened += delta
            }
        }

        guard !submissionScheduled,
              let threshold = Self.submissionThreshold(duration: duration),
              listened >= threshold
        else { return false }
        submissionScheduled = true
        return true
    }

    mutating func finishNaturally() -> Bool {
        guard !submissionScheduled else { return false }
        submissionScheduled = true
        return true
    }

    static func submissionThreshold(duration: TimeInterval) -> TimeInterval? {
        guard duration.isFinite, duration >= 60 else { return nil }
        return min(duration * 0.5, 4 * 60)
    }
}
