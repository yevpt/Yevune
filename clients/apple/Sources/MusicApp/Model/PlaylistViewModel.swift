import CoreFFI
import Foundation

@MainActor
final class PlaylistViewModel: ObservableObject {
    @Published private(set) var tree: PlaylistTree?
    @Published private(set) var detail: PlaylistDetail?
    @Published private(set) var errorMessage: String?

    private let client: any MusicClientProviding

    init(client: any MusicClientProviding) {
        self.client = client
    }

    func loadTree() async {
        errorMessage = nil
        do {
            tree = try await client.playlistTree()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func openPlaylist(id: String) async {
        errorMessage = nil
        do {
            detail = try await client.playlistDetail(id: id)
        } catch {
            errorMessage = error.localizedDescription
        }
    }
}
