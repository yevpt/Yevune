import XCTest
import CoreFFI
@testable import MusicApp

@MainActor
final class PlaylistViewModelTests: XCTestCase {
    func testLoadTreePublishesFoldersAndPlaylists() async {
        let tree = PlaylistTree(
            folders: [PlaylistFolder(id: "folder:1", ownerId: "user:1", name: "Rock", parentId: nil, position: 0)],
            playlists: [playlistFixture(id: "playlist:5", name: "Mix", folderID: "folder:1")]
        )
        let model = PlaylistViewModel(client: FakePlaylistClient(tree: tree))

        await model.loadTree()

        XCTAssertEqual(model.tree?.folders.first?.name, "Rock")
        XCTAssertEqual(model.tree?.playlists.first?.name, "Mix")
        XCTAssertNil(model.errorMessage)
    }

    func testOpenPlaylistLoadsDetail() async {
        let detail = PlaylistDetail(
            playlist: playlistFixture(id: "playlist:5", name: "Mix", folderID: nil),
            tracks: [trackFixture(id: "track:9", title: "Song")]
        )
        let model = PlaylistViewModel(client: FakePlaylistClient(detail: detail))

        await model.openPlaylist(id: "playlist:5")

        XCTAssertEqual(model.detail?.tracks.first?.title, "Song")
    }

    func testCreatePlaylistCallsClientThenReloadsTree() async {
        let fake = FakePlaylistClient()
        let model = PlaylistViewModel(client: fake)

        await model.createPlaylist(name: "New", folderID: "folder:1")

        XCTAssertTrue(fake.calls.contains("create:New:folder:1"))
        XCTAssertEqual(fake.calls.last, "tree") // 创建后整树刷新
        XCTAssertNil(model.errorMessage)
    }

    func testRemoveTrackReloadsOpenDetail() async {
        let detail = PlaylistDetail(
            playlist: playlistFixture(id: "playlist:5", name: "Mix", folderID: nil),
            tracks: [trackFixture(id: "track:9", title: "Song")]
        )
        let fake = FakePlaylistClient(detail: detail)
        let model = PlaylistViewModel(client: fake)
        await model.openPlaylist(id: "playlist:5")

        await model.removeTrack(playlistID: "playlist:5", index: 0)

        XCTAssertTrue(fake.calls.contains("remove:playlist:5:0"))
        XCTAssertEqual(fake.calls.filter { $0 == "detail:playlist:5" }.count, 2) // 打开 + 移除后刷新
    }

    func testSetCommentCallsClientAndRefreshesDetail() async {
        let detail = PlaylistDetail(
            playlist: playlistFixture(id: "playlist:5", name: "Mix", folderID: nil),
            tracks: [trackFixture(id: "track:9", title: "Song")]
        )
        let fake = FakePlaylistClient(detail: detail)
        let model = PlaylistViewModel(client: fake)
        await model.openPlaylist(id: "playlist:5")

        await model.setComment(playlistID: "playlist:5", comment: "hi")

        XCTAssertTrue(fake.calls.contains("comment:playlist:5"))
        XCTAssertEqual(fake.calls.filter { $0 == "detail:playlist:5" }.count, 2) // 打开 + 备注后刷新
    }

    func testDeleteFolderPropagatesError() async {
        let fake = ThrowingPlaylistClient()
        let model = PlaylistViewModel(client: fake)

        await model.deleteFolder(id: "folder:1")

        XCTAssertNotNil(model.errorMessage)
    }
}

@MainActor
func playlistFixture(id: String, name: String, folderID: String?) -> Playlist {
    Playlist(id: id, ownerId: "user:1", name: name, comment: nil, folderId: folderID,
             position: 0, songCount: 0, duration: 0, created: nil, changed: nil)
}

func trackFixture(id: String, title: String) -> Track {
    Track(id: id, title: title, album: nil, albumId: nil, artist: nil, artistId: nil,
          track: nil, discNumber: nil, year: nil, genre: nil, coverArt: nil, size: 0,
          contentType: nil, suffix: nil, duration: 0, bitRate: 0, created: nil)
}

/// 记录调用并返回预设值的歌单假客户端。其余协议方法走默认 featureUnsupported 实现。
final class FakePlaylistClient: MusicClientProviding, @unchecked Sendable {
    var tree: PlaylistTree
    var detail: PlaylistDetail?
    private(set) var calls: [String] = []

    init(tree: PlaylistTree = PlaylistTree(folders: [], playlists: []), detail: PlaylistDetail? = nil) {
        self.tree = tree
        self.detail = detail
    }

    func login(server: String, user: String, password: String) async throws -> SessionValue {
        SessionValue(server: server, user: user)
    }
    func listAlbums(offset: UInt32, size: UInt32) async throws -> [Album] { [] }
    func search(query: String) async throws -> SearchResult { SearchResult(artists: [], albums: [], tracks: []) }
    func upload(localPath: String, libraryKey: String, progress: UploadProgress) async throws -> Track {
        throw CocoaError(.featureUnsupported)
    }
    func updateTags(id: String, update: TagUpdate) async throws {}
    func deleteTrack(id: String) async throws {}
    func moveTrack(id: String, key: String) async throws {}
    func startScan() async throws -> ScanStatus { throw CocoaError(.featureUnsupported) }
    func scanStatus() async throws -> ScanStatus { throw CocoaError(.featureUnsupported) }

    func playlistTree() async throws -> PlaylistTree { calls.append("tree"); return tree }
    func playlistDetail(id: String) async throws -> PlaylistDetail {
        calls.append("detail:\(id)")
        if let detail { return detail }
        return PlaylistDetail(playlist: await playlistFixture(id: id, name: "?", folderID: nil), tracks: [])
    }
    func createPlaylist(name: String, folderID: String?, songIDs: [String]) async throws -> Playlist {
        calls.append("create:\(name):\(folderID ?? "-")")
        return await playlistFixture(id: "playlist:new", name: name, folderID: folderID)
    }
    func renamePlaylist(id: String, name: String) async throws { calls.append("rename:\(id):\(name)") }
    func setPlaylistComment(id: String, comment: String) async throws { calls.append("comment:\(id)") }
    func addTracks(id: String, songIDs: [String]) async throws { calls.append("add:\(id):\(songIDs.joined(separator: ","))") }
    func removeTrackAt(id: String, index: Int64) async throws { calls.append("remove:\(id):\(index)") }
    func deletePlaylist(id: String) async throws { calls.append("delete:\(id)") }
    func movePlaylist(id: String, folderID: String?) async throws { calls.append("move:\(id):\(folderID ?? "-")") }
    func createFolder(name: String, parentID: String?) async throws -> PlaylistFolder {
        calls.append("createFolder:\(name):\(parentID ?? "-")")
        return PlaylistFolder(id: "folder:new", ownerId: "user:1", name: name, parentId: parentID, position: 0)
    }
    func renameFolder(id: String, name: String) async throws { calls.append("renameFolder:\(id):\(name)") }
    func deleteFolder(id: String) async throws { calls.append("deleteFolder:\(id)") }
    func moveFolder(id: String, parentID: String?) async throws { calls.append("moveFolder:\(id):\(parentID ?? "-")") }
}

final class ThrowingPlaylistClient: MusicClientProviding, @unchecked Sendable {
    func login(server: String, user: String, password: String) async throws -> SessionValue {
        SessionValue(server: server, user: user)
    }
    func listAlbums(offset: UInt32, size: UInt32) async throws -> [Album] { [] }
    func search(query: String) async throws -> SearchResult { SearchResult(artists: [], albums: [], tracks: []) }
    func upload(localPath: String, libraryKey: String, progress: UploadProgress) async throws -> Track { throw CocoaError(.featureUnsupported) }
    func updateTags(id: String, update: TagUpdate) async throws {}
    func deleteTrack(id: String) async throws {}
    func moveTrack(id: String, key: String) async throws {}
    func startScan() async throws -> ScanStatus { throw CocoaError(.featureUnsupported) }
    func scanStatus() async throws -> ScanStatus { throw CocoaError(.featureUnsupported) }
    func playlistTree() async throws -> PlaylistTree { PlaylistTree(folders: [], playlists: []) }
    func deleteFolder(id: String) async throws { throw CocoaError(.featureUnsupported) }
    // 其余方法走协议默认 featureUnsupported 实现。
}
