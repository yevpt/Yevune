import SwiftUI

@main
struct MusicApp: App {
    @StateObject private var login: LoginViewModel
    @StateObject private var library: LibraryViewModel
    @StateObject private var upload: UploadViewModel

    init() {
        let client = CoreMusicClient()
        _login = StateObject(wrappedValue: LoginViewModel(client: client))
        _library = StateObject(wrappedValue: LibraryViewModel(client: client))
        _upload = StateObject(wrappedValue: UploadViewModel(client: client))
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
        WindowGroup("上传曲目") {
            UploadView(model: upload)
                .frame(minWidth: 420, minHeight: 260)
        }
    }
}
