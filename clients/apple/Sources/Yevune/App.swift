import SwiftUI

@main
struct YevuneApp: App {
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
            if let session = login.session {
                LibraryView(model: library, session: session)
                    .frame(minWidth: 920, minHeight: 620)
            } else {
                LoginView(model: login)
                    .frame(minWidth: 480, minHeight: 380)
            }
        }
    }
}
