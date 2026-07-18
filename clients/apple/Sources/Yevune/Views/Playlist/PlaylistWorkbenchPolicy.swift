import Foundation
import SwiftUI
import YevuneCoreFFI

struct PlaylistMetadata: Equatable {
    let name: String
    let comment: String
}

enum PlaylistWorkbenchPolicy {
    static func metadata(name: String, comment: String) -> PlaylistMetadata? {
        let name = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !name.isEmpty else { return nil }
        return PlaylistMetadata(
            name: name,
            comment: comment.trimmingCharacters(in: .whitespacesAndNewlines)
        )
    }

    static func moving(
        _ tracks: [Track],
        fromOffsets: IndexSet,
        toOffset: Int
    ) -> [Track] {
        var result = tracks
        result.move(fromOffsets: fromOffsets, toOffset: toOffset)
        return result
    }

    static func removing(_ tracks: [Track], offsets: IndexSet) -> [Track] {
        tracks.enumerated().compactMap { offsets.contains($0.offset) ? nil : $0.element }
    }

    static func selectedTracks(_ tracks: [Track], positions: Set<Int>) -> [Track] {
        tracks.enumerated().compactMap { positions.contains($0.offset) ? $0.element : nil }
    }
}
