import Foundation
import YevuneCoreFFI

enum PlaybackViewPolicy {
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
}
