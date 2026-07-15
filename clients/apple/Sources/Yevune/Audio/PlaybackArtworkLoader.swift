import AppKit
import Foundation

@MainActor
protocol PlaybackArtworkLoading: AnyObject {
    func load(url: URL) async -> NSImage?
}

@MainActor
final class URLSessionPlaybackArtworkLoader: PlaybackArtworkLoading {
    private let session: URLSession

    convenience init() {
        self.init(configuration: Self.makeConfiguration())
    }

    init(configuration: URLSessionConfiguration) {
        session = URLSession(configuration: configuration)
    }

    static func makeConfiguration() -> URLSessionConfiguration {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.urlCache = nil
        configuration.httpCookieStorage = nil
        configuration.urlCredentialStorage = nil
        configuration.httpShouldSetCookies = false
        configuration.httpCookieAcceptPolicy = .never
        configuration.requestCachePolicy = .reloadIgnoringLocalCacheData
        return configuration
    }

    static func makeRequest(url: URL) -> URLRequest {
        var request = URLRequest(
            url: url,
            cachePolicy: .reloadIgnoringLocalCacheData
        )
        request.httpShouldHandleCookies = false
        return request
    }

    func load(url: URL) async -> NSImage? {
        guard let (data, response) = try? await session.data(for: Self.makeRequest(url: url)),
              (response as? HTTPURLResponse)?.statusCode == 200
        else { return nil }
        return NSImage(data: data)
    }
}

@MainActor
final class NoopPlaybackArtworkLoader: PlaybackArtworkLoading {
    func load(url: URL) async -> NSImage? { nil }
}
