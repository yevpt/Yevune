import YevuneCoreFFI
import Foundation

@MainActor
final class MediaViewModel: ObservableObject {
    @Published private(set) var detail: AlbumDetail?
    @Published private(set) var coverURL: URL?
    @Published private(set) var errorMessage: String?
    @Published private(set) var operationMessage: String?
    private let client: any MusicClientProviding

    init(client: any MusicClientProviding) { self.client = client }

    func load(album: Album) async {
        errorMessage = nil
        operationMessage = nil
        do {
            detail = try await client.getAlbum(id: album.id)
            if let cover = album.coverArt {
                coverURL = URL(string: try await client.coverArtURL(id: cover, size: 600))
            } else { coverURL = nil }
        } catch { errorMessage = error.localizedDescription }
    }

    func makeTagEditor(for track: Track) -> TagEditorViewModel {
        TagEditorViewModel(client: client, track: track)
    }

    func refresh(album: Album, successMessage: String) async {
        await load(album: album)
        if errorMessage == nil { operationMessage = successMessage }
    }

    @discardableResult
    func updateTags(ids: [String], update: TagUpdate, album: Album) async -> Int {
        var failures = 0
        for id in ids {
            do { try await client.updateTags(id: id, update: update) }
            catch { failures += 1 }
        }
        await load(album: album)
        if failures > 0 { errorMessage = "\(failures) 项操作失败" }
        else { operationMessage = "已更新 \(ids.count) 首曲目的标签" }
        return failures
    }

    @discardableResult
    func deleteTracks(ids: [String], album: Album) async -> Int {
        var failures = 0
        for id in ids {
            do { try await client.deleteTrack(id: id) }
            catch { failures += 1 }
        }
        await load(album: album)
        if failures > 0 { errorMessage = "\(failures) 项操作失败" }
        else { operationMessage = "已删除 \(ids.count) 首曲目" }
        return failures
    }

    func replaceCover(albumID: String, path: String) async {
        do { try await client.setCoverArt(albumID: albumID, localPath: path) }
        catch { errorMessage = error.localizedDescription }
    }
}
