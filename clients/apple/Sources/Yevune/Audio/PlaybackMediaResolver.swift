import Foundation
import YevuneCoreFFI

struct ResolvedPlaybackMedia: Equatable {
    let streamURL: URL
    let coverURL: URL?
}

enum PlaybackError: LocalizedError {
    case invalidMediaURL

    var errorDescription: String? { "服务器返回了无效的播放地址" }
}

@MainActor
protocol PlaybackMediaResolving: AnyObject {
    func resolve(track: Track) async throws -> ResolvedPlaybackMedia
}

@MainActor
final class MusicClientMediaResolver: PlaybackMediaResolving {
    let client: any MusicClientProviding

    init(client: any MusicClientProviding) {
        self.client = client
    }

    func resolve(track: Track) async throws -> ResolvedPlaybackMedia {
        let stream = try await client.streamURL(trackID: track.id)
        guard let streamURL = URL(string: stream) else {
            throw PlaybackError.invalidMediaURL
        }

        let coverURL: URL?
        if let coverArt = track.coverArt,
           let coverString = try? await client.coverArtURL(id: coverArt, size: 600) {
            coverURL = URL(string: coverString)
        } else {
            coverURL = nil
        }

        return ResolvedPlaybackMedia(streamURL: streamURL, coverURL: coverURL)
    }
}

struct MusicClientPlaybackReporter: PlaybackReporting {
    let client: any PlaybackHistoryProviding

    func reportPlayback(trackID: String, submission: Bool) async throws {
        try await client.scrobble(id: trackID, submission: submission)
    }
}
