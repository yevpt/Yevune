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

@main
struct YevuneApp: App {
    @NSApplicationDelegateAdaptor(ApplicationDelegate.self) private var applicationDelegate
    @StateObject private var login: LoginViewModel
    @StateObject private var library: LibraryAppGraph
    @StateObject private var playback: PlaybackController

    init() {
        let client = CoreMusicClient()
        _login = StateObject(wrappedValue: LoginViewModel(client: client))
        _library = StateObject(wrappedValue: LibraryAppGraph(client: client))
        _playback = StateObject(wrappedValue: PlaybackController(
            resolver: MusicClientMediaResolver(client: client),
            engine: AVQueuePlaybackEngine(),
            systemMedia: SystemMediaCoordinator(),
            artworkLoader: URLSessionPlaybackArtworkLoader()
        ))
    }

    var body: some Scene {
        WindowGroup {
            if let session = login.session {
                LibraryView(
                    client: library.client,
                    browse: library.browse,
                    search: library.search,
                    artistDetail: library.artistDetail,
                    workflow: library.workflow,
                    session: session,
                    playback: playback,
                    onLogout: {
                        playback.shutdown()
                        Task { await login.logout() }
                    }
                )
                    .frame(minWidth: 920, minHeight: 620)
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
