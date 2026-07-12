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

    func createPlaylist(name: String, folderID: String?) async {
        await mutateTree { try await self.client.createPlaylist(name: name, folderID: folderID, songIDs: []) }
    }
    func createFolder(name: String, parentID: String?) async {
        await mutateTree { _ = try await self.client.createFolder(name: name, parentID: parentID) }
    }
    func rename(playlistID: String, name: String) async {
        await mutateTree { try await self.client.renamePlaylist(id: playlistID, name: name) }
    }
    func renameFolder(id: String, name: String) async {
        await mutateTree { try await self.client.renameFolder(id: id, name: name) }
    }
    func delete(playlistID: String) async {
        await mutateTree { try await self.client.deletePlaylist(id: playlistID) }
    }
    func deleteFolder(id: String) async {
        await mutateTree { try await self.client.deleteFolder(id: id) }
    }
    func move(playlistID: String, folderID: String?) async {
        await mutateTree { try await self.client.movePlaylist(id: playlistID, folderID: folderID) }
    }
    func moveFolder(id: String, parentID: String?) async {
        await mutateTree { try await self.client.moveFolder(id: id, parentID: parentID) }
    }
    func addTracks(playlistID: String, songIDs: [String]) async {
        await mutateDetail(playlistID: playlistID) { try await self.client.addTracks(id: playlistID, songIDs: songIDs) }
    }
    func removeTrack(playlistID: String, index: Int64) async {
        await mutateDetail(playlistID: playlistID) { try await self.client.removeTrackAt(id: playlistID, index: index) }
    }
    func setComment(playlistID: String, comment: String) async {
        await mutateDetail(playlistID: playlistID) { try await self.client.setPlaylistComment(id: playlistID, comment: comment) }
    }

    private func mutateTree(_ action: () async throws -> Void) async {
        errorMessage = nil
        do {
            try await action()
            await loadTree()
        } catch {
            // 即使失败也刷新，暴露部分成功的服务端状态；errorMessage 最后设，因 loadTree() 起始会清空它。
            let message = error.localizedDescription
            await loadTree()
            errorMessage = message
        }
    }

    private func mutateDetail(playlistID: String, _ action: () async throws -> Void) async {
        errorMessage = nil
        do {
            try await action()
            if detail?.playlist.id == playlistID { await openPlaylist(id: playlistID) }
            await loadTree() // songCount/duration 可能变化
        } catch {
            // 即使失败也刷新树，暴露部分成功的服务端状态；errorMessage 最后设，因 loadTree() 起始会清空它。
            let message = error.localizedDescription
            await loadTree()
            errorMessage = message
        }
    }
}
