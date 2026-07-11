import SwiftUI

@main
struct MusicApp: App {
    var body: some Scene {
        WindowGroup {
            LoginView(model: LoginViewModel(client: CoreMusicClient()))
                .frame(minWidth: 480, minHeight: 380)
        }
    }
}
