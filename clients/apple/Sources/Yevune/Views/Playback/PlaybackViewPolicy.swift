import Foundation
import YevuneCoreFFI

enum PlaybackViewPolicy {
    enum PrimaryTransportAction: Equatable {
        case play
        case pause
    }

    struct TransportPresentation: Equatable {
        let primaryAction: PrimaryTransportAction
        let showsBufferingIndicator: Bool
        let statusText: String?
        let primaryActionAccessibilityLabel: String
    }

    struct ErrorPresentation: Equatable {
        let message: String

        var accessibilityLabel: String {
            "播放错误：\(message)"
        }
    }

    static func showsPlayerBar(queueCount: Int) -> Bool {
        queueCount > 0
    }

    static let focusedPageShowsQueue = false

    static func hasUpcomingQueueEntries(queueEntryIDs: [UUID], currentID: UUID?) -> Bool {
        guard let currentID,
              let currentIndex = queueEntryIDs.firstIndex(of: currentID)
        else { return false }
        return currentIndex < queueEntryIDs.index(before: queueEntryIDs.endIndex)
    }

    static func transportPresentation(for engineState: PlaybackEngineState) -> TransportPresentation {
        switch engineState {
        case .playing:
            TransportPresentation(
                primaryAction: .pause,
                showsBufferingIndicator: false,
                statusText: nil,
                primaryActionAccessibilityLabel: "暂停"
            )
        case .buffering:
            TransportPresentation(
                primaryAction: .pause,
                showsBufferingIndicator: true,
                statusText: "正在缓冲",
                primaryActionAccessibilityLabel: "暂停（正在缓冲）"
            )
        case .idle, .paused:
            TransportPresentation(
                primaryAction: .play,
                showsBufferingIndicator: false,
                statusText: nil,
                primaryActionAccessibilityLabel: "播放"
            )
        }
    }

    static func errorPresentation(for errorMessage: String?) -> ErrorPresentation? {
        guard let errorMessage, !errorMessage.isEmpty else { return nil }
        return ErrorPresentation(message: errorMessage)
    }

    static func canSeek(duration: TimeInterval) -> Bool {
        duration.isFinite && duration > 0
    }

    static func sliderUpperBound(duration: TimeInterval) -> TimeInterval {
        canSeek(duration: duration) ? duration : 1
    }

    static func progressAccessibilityValue(elapsed: TimeInterval, duration: TimeInterval) -> String? {
        guard canSeek(duration: duration) else { return nil }
        let elapsed = elapsed.isFinite ? min(max(elapsed, 0), duration) : 0
        return "\(formattedTime(elapsed)) / \(formattedTime(duration))"
    }

    static func albumPlaybackOrder(_ tracks: [Track]) -> [Track] {
        tracks.enumerated().sorted { left, right in
            let leftDisc = left.element.discNumber ?? .max
            let rightDisc = right.element.discNumber ?? .max
            if leftDisc != rightDisc { return leftDisc < rightDisc }

            let leftTrack = left.element.track ?? .max
            let rightTrack = right.element.track ?? .max
            if leftTrack != rightTrack { return leftTrack < rightTrack }

            return left.offset < right.offset
        }.map(\.element)
    }

    private static func formattedTime(_ seconds: TimeInterval) -> String {
        let total = Int(seconds.rounded(.down))
        return String(format: "%d:%02d", total / 60, total % 60)
    }
}
