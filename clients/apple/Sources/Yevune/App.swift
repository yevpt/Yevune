import SwiftUI

@MainActor
final class LibraryAppGraph: ObservableObject {
    let client: any MusicClientProviding
    let browse: LibraryBrowseViewModel
    let search: LibrarySearchViewModel
    let artistDetail: ArtistDetailViewModel
    let workflow: LibraryWorkflowViewModel

    init(client: any MusicClientProviding) {
        self.client = client
        let browse = LibraryBrowseViewModel(client: client)
        self.browse = browse
        search = LibrarySearchViewModel(client: client)
        artistDetail = ArtistDetailViewModel(client: client)
        workflow = LibraryWorkflowViewModel(client: client, library: browse)
    }
}

@MainActor
final class AuthenticatedLibraryGraphOwner: ObservableObject {
    let graph: LibraryAppGraph

    init(client: any MusicClientProviding, session _: SessionValue) {
        graph = LibraryAppGraph(client: client)
    }
}

private struct AuthenticatedLibraryRoot: View {
    @StateObject private var owner: AuthenticatedLibraryGraphOwner
    @ObservedObject var playback: PlaybackController
    let session: SessionValue
    let onLogout: () -> Void

    init(
        client: any MusicClientProviding,
        session: SessionValue,
        playback: PlaybackController,
        onLogout: @escaping () -> Void
    ) {
        _owner = StateObject(
            wrappedValue: AuthenticatedLibraryGraphOwner(client: client, session: session)
        )
        self.session = session
        self.playback = playback
        self.onLogout = onLogout
    }

    var body: some View {
        LibraryView(
            client: owner.graph.client,
            browse: owner.graph.browse,
            search: owner.graph.search,
            artistDetail: owner.graph.artistDetail,
            workflow: owner.graph.workflow,
            session: session,
            playback: playback,
            onLogout: onLogout
        )
        .frame(minWidth: 920, minHeight: 620)
    }
}

@main
struct YevuneApp: App {
    @NSApplicationDelegateAdaptor(ApplicationDelegate.self) private var applicationDelegate
    @StateObject private var login: LoginViewModel
    @StateObject private var playback: PlaybackController
    private let client: CoreMusicClient

    init() {
        let client = CoreMusicClient()
        self.client = client
        _login = StateObject(wrappedValue: LoginViewModel(client: client))
        _playback = StateObject(wrappedValue: PlaybackController(
            resolver: MusicClientMediaResolver(client: client),
            engine: AVQueuePlaybackEngine(),
            reporter: MusicClientPlaybackReporter(client: client),
            systemMedia: SystemMediaCoordinator(),
            artworkLoader: URLSessionPlaybackArtworkLoader()
        ))
    }

    var body: some Scene {
        WindowGroup {
            if let session = login.session {
                AuthenticatedLibraryRoot(
                    client: client,
                    session: session,
                    playback: playback,
                    onLogout: {
                        playback.shutdown()
                        Task { await login.logout() }
                    }
                )
            } else {
                LoginView(model: login)
                    .frame(minWidth: 480, minHeight: 380)
            }
        }

        Window("迷你播放器", id: "mini-player") {
            MiniPlayerView(playback: playback)
                .frame(width: 360, height: 132)
        }
        .windowResizability(.contentSize)
    }
}
