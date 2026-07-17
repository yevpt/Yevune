import Foundation
import YevuneCoreFFI

enum AlbumWorkbenchColumn: Equatable {
    case trackNumber
    case titleAndArtist
    case title
    case artist
    case duration
    case format
}

enum AlbumManagementAction: Equatable {
    case editTags
    case replaceCover
    case move
    case delete
    case manageAccess
}

struct AlbumWorkbenchGridMetrics: Equatable {
    let outerHorizontalInset: CGFloat
    let horizontalSpacing: CGFloat
    let playButtonWidth: CGFloat
    let trackNumberWidth: CGFloat
    let artworkSize: CGFloat
    let headerSpacing: CGFloat

    var titleLeadingOffset: CGFloat {
        playButtonWidth + horizontalSpacing + trackNumberWidth + horizontalSpacing
    }

    var trackGridLeadingEdge: CGFloat { outerHorizontalInset }

    var metadataLeadingEdge: CGFloat {
        outerHorizontalInset + artworkSize + headerSpacing
    }

    func contentTrailingEdge(width: CGFloat) -> CGFloat {
        width - outerHorizontalInset
    }
}

struct AlbumDiscGroup: Equatable {
    let discNumber: UInt32
    let tracks: [Track]
}

enum AlbumWorkbenchPolicy {
    static func columns(width: CGFloat) -> [AlbumWorkbenchColumn] {
        width >= 620
            ? [.trackNumber, .title, .artist, .duration, .format]
            : [.trackNumber, .titleAndArtist, .duration]
    }

    static func managementActions(isAdmin: Bool) -> [AlbumManagementAction] {
        isAdmin ? [.editTags, .replaceCover, .move, .delete, .manageAccess] : []
    }

    static func gridMetrics(width: CGFloat) -> AlbumWorkbenchGridMetrics {
        let isWide = width >= 620
        return AlbumWorkbenchGridMetrics(
            outerHorizontalInset: 12,
            horizontalSpacing: 12,
            playButtonWidth: 18,
            trackNumberWidth: columns(width: width).contains(.trackNumber) ? 44 : 0,
            artworkSize: isWide ? 200 : 144,
            headerSpacing: isWide ? 24 : 16
        )
    }

    static func metadata(album: Album, tracks: [Track]) -> String {
        var parts: [String] = []
        if let year = album.year {
            parts.append(String(year))
        }
        if let genre = album.genre?.trimmingCharacters(in: .whitespacesAndNewlines), !genre.isEmpty {
            parts.append(genre)
        }
        parts.append("\(album.songCount) 首")

        let loadedDuration = tracks.reduce(UInt64(0)) { $0 + UInt64($1.duration) }
        if loadedDuration > 0 {
            parts.append(formattedDuration(loadedDuration))
        }
        return parts.joined(separator: " · ")
    }

    static func trackNumber(_ track: Track, isMultiDisc: Bool) -> String {
        guard let number = track.track else { return "—" }
        let paddedTrack = String(format: "%02u", number)
        guard isMultiDisc else { return paddedTrack }
        return "\(normalizedDiscNumber(track))·\(paddedTrack)"
    }

    static func normalizedDiscNumber(_ track: Track) -> UInt32 {
        track.discNumber ?? 1
    }

    static func isMultiDisc(_ tracks: [Track]) -> Bool {
        Set(tracks.map(normalizedDiscNumber)).count > 1
    }

    static func discGroups(_ tracks: [Track]) -> [AlbumDiscGroup] {
        let groups = Dictionary(grouping: tracks, by: normalizedDiscNumber)
        return groups.keys.sorted().map { discNumber in
            AlbumDiscGroup(discNumber: discNumber, tracks: groups[discNumber] ?? [])
        }
    }

    static func reconciledSelection(_ selection: Set<String>, tracks: [Track]) -> Set<String> {
        reconciledSelection(selection, trackIDs: tracks.map(\.id))
    }

    static func reconciledSelection(_ selection: Set<String>, trackIDs: [String]) -> Set<String> {
        selection.intersection(trackIDs)
    }

    static func emptyMessage(isAdmin: Bool) -> String {
        isAdmin
            ? "此专辑暂无曲目，可通过曲库导入添加音乐。"
            : "此专辑暂无可播放的曲目。"
    }

    private static func formattedDuration(_ seconds: UInt64) -> String {
        let hours = seconds / 3_600
        let minutes = seconds % 3_600 / 60
        let remainingSeconds = seconds % 60
        if hours > 0 {
            return String(format: "%llu:%02llu:%02llu", hours, minutes, remainingSeconds)
        }
        return String(format: "%llu:%02llu", minutes, remainingSeconds)
    }
}
