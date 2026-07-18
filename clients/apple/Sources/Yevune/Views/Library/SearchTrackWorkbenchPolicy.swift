import YevuneCoreFFI

enum SearchTrackWorkbenchPolicy {
    static func selectedTracks(_ tracks: [Track], selection: Set<String>) -> [Track] {
        tracks.filter { selection.contains($0.id) }
    }

    static func reconciledSelection(_ selection: Set<String>, tracks: [Track]) -> Set<String> {
        selection.intersection(tracks.map(\.id))
    }

    static func selectAll(_ tracks: [Track]) -> Set<String> {
        Set(tracks.map(\.id))
    }
}
