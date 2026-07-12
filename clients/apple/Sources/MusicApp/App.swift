import SwiftUI

@main
struct MusicApp: App {
    @NSApplicationDelegateAdaptor(ApplicationDelegate.self) private var applicationDelegate
    @StateObject private var login: LoginViewModel
    @StateObject private var library: LibraryViewModel

    init() {
        let client = CoreMusicClient()
        _login = StateObject(wrappedValue: LoginViewModel(client: client))
        _library = StateObject(wrappedValue: LibraryViewModel(client: client))
    }

    var body: some Scene {
        WindowGroup {
            if login.session == nil {
                LoginView(model: login)
                    .frame(minWidth: 480, minHeight: 380)
            } else {
                LibraryView(model: library)
                    .frame(minWidth: 920, minHeight: 620)
            }
        }
    }
}
