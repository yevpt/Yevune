import YevuneCoreFFI
import Foundation

@MainActor
final class PlaylistViewModel: ObservableObject {
    @Published private(set) var tree: PlaylistTree?
    @Published private(set) var detail: PlaylistDetail?
    @Published private(set) var errorMessage: String?
    @Published private(set) var isMutating = false
    @Published private(set) var isLoadingDetail = false

    private let client: any MusicClientProviding
    private var detailGeneration = 0

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
        detailGeneration += 1
        let generation = detailGeneration
        errorMessage = nil
        isLoadingDetail = true
        do {
            let loaded = try await client.playlistDetail(id: id)
            guard generation == detailGeneration else { return }
            detail = loaded
        } catch {
            guard generation == detailGeneration else { return }
            errorMessage = error.localizedDescription
        }
        if generation == detailGeneration { isLoadingDetail = false }
    }

    func createPlaylist(name: String, folderID: String?) async {
        await mutateTree { _ = try await self.client.createPlaylist(name: name, folderID: folderID, songIDs: []) }
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
    @discardableResult
    func addTracks(playlistID: String, songIDs: [String]) async -> Bool {
        guard !isMutating, !songIDs.isEmpty else { return false }
        isMutating = true
        errorMessage = nil
        let generation = detailGeneration
        defer { isMutating = false }
        do {
            try await client.addTracks(id: playlistID, songIDs: songIDs)
        } catch {
            errorMessage = error.localizedDescription
            return false
        }

        var refreshError: String?
        if detail?.playlist.id == playlistID {
            do {
                let refreshed = try await client.playlistDetail(id: playlistID)
                if generation == detailGeneration, detail?.playlist.id == playlistID {
                    detail = refreshed
                }
            } catch {
                refreshError = error.localizedDescription
            }
        }
        do {
            tree = try await client.playlistTree()
        } catch {
            refreshError = refreshError ?? error.localizedDescription
        }
        errorMessage = refreshError
        return true
    }
    func removeTrack(playlistID: String, index: Int64) async {
        await mutateDetail(playlistID: playlistID) { try await self.client.removeTrackAt(id: playlistID, index: index) }
    }
    func setComment(playlistID: String, comment: String) async {
        await mutateDetail(playlistID: playlistID) { try await self.client.setPlaylistComment(id: playlistID, comment: comment) }
    }

    @discardableResult
    func saveMetadata(playlistID: String, name: String, comment: String) async -> Bool {
        guard !isMutating,
              let metadata = PlaylistWorkbenchPolicy.metadata(name: name, comment: comment)
        else { return false }
        isMutating = true
        errorMessage = nil
        let generation = detailGeneration
        defer { isMutating = false }
        do {
            try await client.updatePlaylistMetadata(
                id: playlistID,
                name: metadata.name,
                comment: metadata.comment
            )
            let refreshedDetail = try await client.playlistDetail(id: playlistID)
            if generation == detailGeneration, detail?.playlist.id == playlistID {
                detail = refreshedDetail
            }
            tree = try await client.playlistTree()
            return true
        } catch {
            if generation == detailGeneration { errorMessage = error.localizedDescription }
            return false
        }
    }

    @discardableResult
    func replaceTracks(playlistID: String, tracks: [Track]) async -> Bool {
        guard !isMutating,
              let previous = detail,
              previous.playlist.id == playlistID
        else { return false }
        isMutating = true
        errorMessage = nil
        let generation = detailGeneration
        detail = PlaylistDetail(playlist: previous.playlist, tracks: tracks)
        defer { isMutating = false }
        do {
            let replaced = try await client.replacePlaylistTracks(
                id: playlistID,
                songIDs: tracks.map(\.id)
            )
            if generation == detailGeneration, detail?.playlist.id == playlistID {
                detail = replaced
            }
            tree = try await client.playlistTree()
            return true
        } catch {
            if generation == detailGeneration, detail?.playlist.id == playlistID {
                detail = previous
                errorMessage = error.localizedDescription
            }
            return false
        }
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
