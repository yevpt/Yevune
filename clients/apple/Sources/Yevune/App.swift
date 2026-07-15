import SwiftUI

@main
struct YevuneApp: App {
    @NSApplicationDelegateAdaptor(ApplicationDelegate.self) private var applicationDelegate
    @StateObject private var login: LoginViewModel
    @StateObject private var library: LibraryViewModel
    @StateObject private var playback: PlaybackController

    init() {
        let client = CoreMusicClient()
        _login = StateObject(wrappedValue: LoginViewModel(client: client))
        _library = StateObject(wrappedValue: LibraryViewModel(client: client))
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
                    model: library,
                    session: session,
                    playback: playback,
                    onLogout: {
                        playback.shutdown()
                        login.logout()
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
