import XCTest
import YevuneCoreFFI
@testable import Yevune

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

    func testLatePlaylistSuccessCannotOverwriteNewSelection() async {
        let client = SuspendedPlaylistClient()
        let model = PlaylistViewModel(client: client)

        let first = Task { await model.openPlaylist(id: "playlist:first") }
        await client.waitForCallCount(1)
        let second = Task { await model.openPlaylist(id: "playlist:second") }
        await client.waitForCallCount(2)

        await client.resolveCall(1, with: playlistDetailFixture(id: "playlist:second", name: "Second"))
        await second.value
        await client.resolveCall(0, with: playlistDetailFixture(id: "playlist:first", name: "First"))
        await first.value

        XCTAssertEqual(model.detail?.playlist.id, "playlist:second")
        XCTAssertFalse(model.isLoadingDetail)
        XCTAssertNil(model.errorMessage)
    }

    func testFailedOpenStopsLoadingAndPublishesError() async {
        let model = PlaylistViewModel(client: ThrowingPlaylistClient())

        await model.openPlaylist(id: "playlist:missing")

        XCTAssertFalse(model.isLoadingDetail)
        XCTAssertNotNil(model.errorMessage)
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

    func testAddTracksSubmitsAllSelectedTrackIDsInOneCall() async {
        let fake = FakePlaylistClient()
        let model = PlaylistViewModel(client: fake)

        let added = await model.addTracks(
            playlistID: "playlist:5",
            songIDs: ["track:1", "track:2"]
        )

        XCTAssertTrue(added)
        XCTAssertTrue(fake.calls.contains("add:playlist:5:track:1,track:2"))
        XCTAssertEqual(fake.calls.last, "tree")
    }

    func testAddTracksFailureReturnsFalseAndPublishesError() async {
        let model = PlaylistViewModel(client: ThrowingPlaylistClient())

        let added = await model.addTracks(playlistID: "playlist:5", songIDs: ["track:1"])

        XCTAssertFalse(added)
        XCTAssertNotNil(model.errorMessage)
        XCTAssertFalse(model.isMutating)
    }

    func testAddTracksIsSingleFlight() async {
        let client = SuspendedPlaylistClient()
        let model = PlaylistViewModel(client: client)

        let first = Task {
            await model.addTracks(playlistID: "playlist:5", songIDs: ["track:1", "track:2"])
        }
        await client.waitForAddTracks()
        let duplicate = await model.addTracks(playlistID: "playlist:5", songIDs: ["track:3"])

        XCTAssertFalse(duplicate)
        XCTAssertTrue(model.isMutating)
        await client.resolveAddTracks()
        let firstSucceeded = await first.value
        let addCount = await client.addTracksCallCount()
        XCTAssertTrue(firstSucceeded)
        XCTAssertEqual(addCount, 1)
        XCTAssertFalse(model.isMutating)
    }

    func testDeleteFolderPropagatesError() async {
        let fake = ThrowingPlaylistClient()
        let model = PlaylistViewModel(client: fake)

        await model.deleteFolder(id: "folder:1")

        XCTAssertNotNil(model.errorMessage)
    }

    func testSaveMetadataUsesSingleRequestAndRefreshesSharedTreeAndDetail() async {
        let detail = PlaylistDetail(
            playlist: playlistFixture(id: "playlist:5", name: "Mix", folderID: nil),
            tracks: [trackFixture(id: "track:1", title: "One")]
        )
        let fake = FakePlaylistClient(detail: detail)
        let model = PlaylistViewModel(client: fake)
        await model.openPlaylist(id: "playlist:5")

        let saved = await model.saveMetadata(
            playlistID: "playlist:5",
            name: "Road Trip",
            comment: "Night drive"
        )

        XCTAssertTrue(saved)
        XCTAssertTrue(fake.calls.contains("metadata:playlist:5:Road Trip:Night drive"))
        XCTAssertEqual(fake.calls.filter { $0 == "detail:playlist:5" }.count, 2)
        XCTAssertEqual(fake.calls.last, "tree")
        XCTAssertFalse(model.isMutating)
    }

    func testReplaceTracksPreservesRepeatedInstancesAndUsesReturnedDetail() async {
        let repeated = trackFixture(id: "track:1", title: "Repeat")
        let tail = trackFixture(id: "track:2", title: "Tail")
        let original = PlaylistDetail(
            playlist: playlistFixture(id: "playlist:5", name: "Mix", folderID: nil),
            tracks: [repeated, tail, repeated]
        )
        let replacement = PlaylistDetail(
            playlist: original.playlist,
            tracks: [tail, repeated, repeated]
        )
        let fake = FakePlaylistClient(detail: original, replacementDetail: replacement)
        let model = PlaylistViewModel(client: fake)
        await model.openPlaylist(id: "playlist:5")

        let replaced = await model.replaceTracks(
            playlistID: "playlist:5",
            tracks: replacement.tracks
        )

        XCTAssertTrue(replaced)
        XCTAssertTrue(fake.calls.contains("replace:playlist:5:track:2,track:1,track:1"))
        XCTAssertEqual(model.detail?.tracks.map(\.title), ["Tail", "Repeat", "Repeat"])
        XCTAssertEqual(fake.calls.last, "tree")
    }

    func testFailedReplacementRollsBackOptimisticOrder() async {
        let first = trackFixture(id: "track:1", title: "First")
        let second = trackFixture(id: "track:2", title: "Second")
        let original = PlaylistDetail(
            playlist: playlistFixture(id: "playlist:5", name: "Mix", folderID: nil),
            tracks: [first, second]
        )
        let fake = FakePlaylistClient(detail: original, replacementShouldFail: true)
        let model = PlaylistViewModel(client: fake)
        await model.openPlaylist(id: "playlist:5")

        let replaced = await model.replaceTracks(
            playlistID: "playlist:5",
            tracks: [second, first]
        )

        XCTAssertFalse(replaced)
        XCTAssertEqual(model.detail?.tracks.map(\.title), ["First", "Second"])
        XCTAssertNotNil(model.errorMessage)
        XCTAssertFalse(model.isMutating)
    }

    func testReplacementIsSingleFlight() async {
        let original = playlistDetailFixture(id: "playlist:5", name: "Mix")
        let client = SuspendedPlaylistClient(immediateDetail: original)
        let model = PlaylistViewModel(client: client)
        await model.openPlaylist(id: "playlist:5")

        let first = Task {
            await model.replaceTracks(playlistID: "playlist:5", tracks: original.tracks)
        }
        await client.waitForReplacement()
        let duplicate = await model.replaceTracks(playlistID: "playlist:5", tracks: original.tracks)

        XCTAssertFalse(duplicate)
        XCTAssertTrue(model.isMutating)
        await client.resolveReplacement(with: original)
        let firstSucceeded = await first.value
        let replacementCount = await client.replacementCallCount()
        XCTAssertTrue(firstSucceeded)
        XCTAssertEqual(replacementCount, 1)
        XCTAssertFalse(model.isMutating)
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
          contentType: nil, suffix: nil, duration: 0, bitRate: 0, created: nil, path: nil)
}

@MainActor
private func playlistDetailFixture(id: String, name: String) -> PlaylistDetail {
    PlaylistDetail(playlist: playlistFixture(id: id, name: name, folderID: nil), tracks: [])
}

/// 记录调用并返回预设值的歌单假客户端。其余协议方法走默认 featureUnsupported 实现。
final class FakePlaylistClient: MusicClientProviding, @unchecked Sendable {
    var tree: PlaylistTree
    var detail: PlaylistDetail?
    private(set) var calls: [String] = []
    var replacementDetail: PlaylistDetail?
    var replacementShouldFail: Bool

    init(
        tree: PlaylistTree = PlaylistTree(folders: [], playlists: []),
        detail: PlaylistDetail? = nil,
        replacementDetail: PlaylistDetail? = nil,
        replacementShouldFail: Bool = false
    ) {
        self.tree = tree
        self.detail = detail
        self.replacementDetail = replacementDetail
        self.replacementShouldFail = replacementShouldFail
    }

    func login(server: String, user: String, password: String) async throws -> SessionValue {
        SessionValue(server: server, user: user)
    }
    func logout() async {}
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
    func updatePlaylistMetadata(id: String, name: String, comment: String) async throws {
        calls.append("metadata:\(id):\(name):\(comment)")
    }
    func replacePlaylistTracks(id: String, songIDs: [String]) async throws -> PlaylistDetail {
        calls.append("replace:\(id):\(songIDs.joined(separator: ","))")
        if replacementShouldFail { throw CocoaError(.fileWriteUnknown) }
        if let replacementDetail { return replacementDetail }
        if let detail { return detail }
        return PlaylistDetail(
            playlist: await playlistFixture(id: id, name: "?", folderID: nil),
            tracks: []
        )
    }
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
    func logout() async {}
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

private actor SuspendedPlaylistClient: MusicClientProviding {
    private let immediateDetail: PlaylistDetail?
    private var continuations: [CheckedContinuation<PlaylistDetail, Error>] = []
    private var callCountWaiters: [(Int, CheckedContinuation<Void, Never>)] = []
    private var replacementContinuation: CheckedContinuation<PlaylistDetail, Error>?
    private var replacementWaiters: [CheckedContinuation<Void, Never>] = []
    private var replacements = 0
    private var addTracksContinuation: CheckedContinuation<Void, Error>?
    private var addTracksWaiters: [CheckedContinuation<Void, Never>] = []
    private var addCalls = 0

    init(immediateDetail: PlaylistDetail? = nil) {
        self.immediateDetail = immediateDetail
    }

    func playlistDetail(id _: String) async throws -> PlaylistDetail {
        if let immediateDetail { return immediateDetail }
        let callCount = continuations.count + 1
        resumeSatisfiedWaiters(for: callCount)
        return try await withCheckedThrowingContinuation { continuations.append($0) }
    }

    func waitForCallCount(_ count: Int) async {
        guard continuations.count < count else { return }
        await withCheckedContinuation { callCountWaiters.append((count, $0)) }
    }

    func resolveCall(_ index: Int, with detail: PlaylistDetail) {
        continuations[index].resume(returning: detail)
    }

    func replacePlaylistTracks(id _: String, songIDs _: [String]) async throws -> PlaylistDetail {
        replacements += 1
        replacementWaiters.forEach { $0.resume() }
        replacementWaiters.removeAll()
        return try await withCheckedThrowingContinuation { replacementContinuation = $0 }
    }

    func waitForReplacement() async {
        guard replacements == 0 else { return }
        await withCheckedContinuation { replacementWaiters.append($0) }
    }

    func resolveReplacement(with detail: PlaylistDetail) {
        replacementContinuation?.resume(returning: detail)
        replacementContinuation = nil
    }

    func replacementCallCount() -> Int { replacements }

    func addTracks(id _: String, songIDs _: [String]) async throws {
        addCalls += 1
        addTracksWaiters.forEach { $0.resume() }
        addTracksWaiters.removeAll()
        try await withCheckedThrowingContinuation { addTracksContinuation = $0 }
    }

    func waitForAddTracks() async {
        guard addCalls == 0 else { return }
        await withCheckedContinuation { addTracksWaiters.append($0) }
    }

    func resolveAddTracks() {
        addTracksContinuation?.resume()
        addTracksContinuation = nil
    }

    func addTracksCallCount() -> Int { addCalls }

    func playlistTree() async throws -> PlaylistTree {
        PlaylistTree(folders: [], playlists: [])
    }

    private func resumeSatisfiedWaiters(for count: Int) {
        let satisfied = callCountWaiters.filter { count >= $0.0 }
        callCountWaiters.removeAll { count >= $0.0 }
        satisfied.forEach { $0.1.resume() }
    }

    func login(server _: String, user _: String, password _: String) async throws -> SessionValue {
        throw CocoaError(.featureUnsupported)
    }
    func logout() async {}
    func listAlbums(offset _: UInt32, size _: UInt32) async throws -> [Album] { [] }
    func search(query _: String) async throws -> SearchResult { SearchResult(artists: [], albums: [], tracks: []) }
    func upload(localPath _: String, libraryKey _: String, progress _: UploadProgress) async throws -> Track {
        throw CocoaError(.featureUnsupported)
    }
    func updateTags(id _: String, update _: TagUpdate) async throws {}
    func deleteTrack(id _: String) async throws {}
    func moveTrack(id _: String, key _: String) async throws {}
    func startScan() async throws -> ScanStatus { throw CocoaError(.featureUnsupported) }
    func scanStatus() async throws -> ScanStatus { throw CocoaError(.featureUnsupported) }
}
