import AppKit
import SwiftUI

struct AuthenticatedArtworkRequest: Equatable {
    let url: URL?
    let revision: Int
}

enum AuthenticatedArtworkLoadOutcome: Equatable {
    case loaded
    case failed
    case superseded
}

enum AuthenticatedArtworkLoadState: Equatable {
    case idle
    case loading(AuthenticatedArtworkRequest)
    case loaded(AuthenticatedArtworkRequest)
    case failed(AuthenticatedArtworkRequest)
}

@MainActor
final class AuthenticatedArtworkLoaderModel: ObservableObject {
    @Published private(set) var image: NSImage?
    @Published private(set) var state: AuthenticatedArtworkLoadState = .idle

    private let loader: any PlaybackArtworkLoading
    private var generation = 0

    init(loader: any PlaybackArtworkLoading) {
        self.loader = loader
    }

    func load(_ request: AuthenticatedArtworkRequest) async -> AuthenticatedArtworkLoadOutcome {
        generation += 1
        let requestGeneration = generation
        image = nil
        state = .loading(request)

        guard let url = request.url else {
            state = .failed(request)
            return .failed
        }
        let loaded = await loader.load(url: url)
        guard !Task.isCancelled, requestGeneration == generation else { return .superseded }
        guard let loaded else {
            state = .failed(request)
            return .failed
        }

        image = loaded
        state = .loaded(request)
        return .loaded
    }
}

/// Renders already-decoded artwork without giving an authenticated URL to
/// SwiftUI's shared image-loading stack.
struct DecodedArtworkView<Placeholder: View>: View {
    let image: NSImage?
    @ViewBuilder let placeholder: () -> Placeholder

    var body: some View {
        if let image {
            Image(nsImage: image)
                .resizable()
                .scaledToFill()
        } else {
            placeholder()
        }
    }
}

/// Loads authenticated cover URLs through the same ephemeral, cache-free
/// loader used by playback metadata. URL plus revision identifies each load;
/// generation and cancellation prevent late results from replacing newer art.
struct AuthenticatedArtworkView<Placeholder: View>: View {
    let url: URL?
    let revision: Int
    let onLoad: (Int, AuthenticatedArtworkLoadOutcome) -> Void
    @ViewBuilder let placeholder: () -> Placeholder
    @StateObject private var model: AuthenticatedArtworkLoaderModel

    init(
        url: URL?,
        revision: Int = 0,
        onLoad: @escaping (Int, AuthenticatedArtworkLoadOutcome) -> Void = { _, _ in },
        @ViewBuilder placeholder: @escaping () -> Placeholder
    ) {
        self.url = url
        self.revision = revision
        self.onLoad = onLoad
        self.placeholder = placeholder
        _model = StateObject(
            wrappedValue: AuthenticatedArtworkLoaderModel(loader: URLSessionPlaybackArtworkLoader())
        )
    }

    var body: some View {
        let request = AuthenticatedArtworkRequest(url: url, revision: revision)
        DecodedArtworkView(image: model.image, placeholder: placeholder)
            .task(id: request) {
                let outcome = await model.load(request)
                onLoad(revision, outcome)
            }
    }
}
