import Foundation
import YevuneCoreFFI

struct QueueEntry: Identifiable {
    let id: UUID
    let track: Track

    init(id: UUID = UUID(), track: Track) {
        self.id = id
        self.track = track
    }
}

enum PlaybackRepeatMode: String, CaseIterable {
    case off, all, one
}

struct PlaybackQueue {
    private(set) var entries: [QueueEntry] = []
    private(set) var currentIndex: Int?
    private var originalEntries: [QueueEntry] = []
    private(set) var isShuffled = false
    var repeatMode: PlaybackRepeatMode = .off

    var current: QueueEntry? {
        guard let currentIndex, entries.indices.contains(currentIndex) else { return nil }
        return entries[currentIndex]
    }

    mutating func replace(with tracks: [Track], startingAt index: Int) {
        let newEntries = tracks.map { QueueEntry(track: $0) }
        entries = newEntries
        originalEntries = newEntries
        currentIndex = newEntries.indices.contains(index) ? index : newEntries.indices.first
        isShuffled = false
    }

    mutating func insertNext(_ track: Track) {
        let target = min((currentIndex ?? -1) + 1, entries.count)
        let entry = QueueEntry(track: track)
        entries.insert(entry, at: target)
        originalEntries.append(entry)
    }

    mutating func append(_ track: Track) {
        let entry = QueueEntry(track: track)
        entries.append(entry)
        originalEntries.append(entry)
        if currentIndex == nil { currentIndex = 0 }
    }

    mutating func move(from source: Int, to destination: Int) {
        guard entries.indices.contains(source), destination >= 0, destination < entries.count else { return }
        let currentID = current?.id
        let entry = entries.remove(at: source)
        entries.insert(entry, at: destination)
        currentIndex = currentID.flatMap { id in entries.firstIndex { $0.id == id } }
    }

    mutating func remove(id: UUID) {
        guard let index = entries.firstIndex(where: { $0.id == id }) else { return }
        let wasCurrent = index == currentIndex
        entries.remove(at: index)
        originalEntries.removeAll { $0.id == id }
        if entries.isEmpty { currentIndex = nil }
        else if wasCurrent { currentIndex = min(index, entries.count - 1) }
        else if let currentIndex, index < currentIndex { self.currentIndex = currentIndex - 1 }
    }

    mutating func previous() -> QueueEntry? {
        guard let currentIndex, currentIndex > 0 else { return nil }
        self.currentIndex = currentIndex - 1
        return current
    }

    mutating func nextAfterManualSkip() -> QueueEntry? {
        advance(wrap: repeatMode == .all)
    }

    mutating func nextAfterNaturalEnd() -> QueueEntry? {
        if repeatMode == .one { return current }
        return advance(wrap: repeatMode == .all)
    }

    mutating func setShuffled(_ enabled: Bool, using shuffle: ([QueueEntry]) -> [QueueEntry]) {
        guard enabled != isShuffled, let currentIndex else { return }
        let throughCurrent = Array(entries[...currentIndex])
        let used = Set(throughCurrent.map(\.id))
        let originalRemaining = originalEntries.filter { !used.contains($0.id) }
        entries = throughCurrent + (enabled ? shuffle(originalRemaining) : originalRemaining)
        self.currentIndex = throughCurrent.count - 1
        isShuffled = enabled
    }

    private mutating func advance(wrap: Bool) -> QueueEntry? {
        guard let currentIndex, !entries.isEmpty else { return nil }
        if currentIndex + 1 < entries.count { self.currentIndex = currentIndex + 1 }
        else if wrap { self.currentIndex = 0 }
        else { return nil }
        return current
    }
}
