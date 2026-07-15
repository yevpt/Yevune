import AppKit
import Foundation

@MainActor
protocol PlaybackArtworkLoading: AnyObject {
    func load(url: URL) async -> NSImage?
}

@MainActor
final class URLSessionPlaybackArtworkLoader: PlaybackArtworkLoading {
    func load(url: URL) async -> NSImage? {
        guard let (data, response) = try? await URLSession.shared.data(from: url),
              (response as? HTTPURLResponse)?.statusCode == 200
        else { return nil }
        return NSImage(data: data)
    }
}

@MainActor
final class NoopPlaybackArtworkLoader: PlaybackArtworkLoading {
    func load(url: URL) async -> NSImage? { nil }
}
