import AppKit
import SwiftUI

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
/// loader used by playback metadata. Late results are gated by SwiftUI's task
/// identity and cancellation.
struct AuthenticatedArtworkView<Placeholder: View>: View {
    let url: URL?
    @ViewBuilder let placeholder: () -> Placeholder
    @State private var image: NSImage?

    var body: some View {
        DecodedArtworkView(image: image, placeholder: placeholder)
            .task(id: url) {
                image = nil
                guard let url else { return }
                let loaded = await URLSessionPlaybackArtworkLoader().load(url: url)
                guard !Task.isCancelled else { return }
                image = loaded
            }
    }
}
