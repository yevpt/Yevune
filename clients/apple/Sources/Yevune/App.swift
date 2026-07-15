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
            engine: AVQueuePlaybackEngine()
        ))
    }

    var body: some Scene {
        WindowGroup {
            if let session = login.session {
                LibraryView(model: library, session: session, playback: playback)
                    .frame(minWidth: 920, minHeight: 620)
            } else {
                LoginView(model: login)
                    .frame(minWidth: 480, minHeight: 380)
            }
        }
    }
}
