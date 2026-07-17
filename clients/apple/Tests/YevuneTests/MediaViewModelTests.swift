import Foundation
import YevuneCoreFFI
import XCTest
@testable import Yevune

@MainActor
final class MediaViewModelTests: XCTestCase {
    func testInitialLoadPublishesLoadingThenContent() async {
        let client = SuspendedAlbumClient()
        let model = MediaViewModel(client: client)

        let load = Task { await model.load(album: album("a")) }
        await client.waitForAlbumCalls(1)
        XCTAssertEqual(model.phase, .loading)

        await client.resolveAlbumCall(0, with: detail("a"))
        await load.value

        XCTAssertEqual(model.phase, .content)
        XCTAssertEqual(model.detail?.album.id, "a")
    }

    func testLateAlbumAResponseCannotOverwriteAlbumB() async {
        let client = SuspendedAlbumClient()
        let model = MediaViewModel(client: client)

        let a = Task { await model.load(album: album("a")) }
        await client.waitForAlbumCalls(1)
        let b = Task { await model.load(album: album("b")) }
        await client.waitForAlbumCalls(2)

        await client.resolveAlbumCall(1, with: detail("b"))
        await client.resolveAlbumCall(0, with: detail("a"))
        await a.value
        await b.value

        XCTAssertEqual(model.currentAlbumID, "b")
        XCTAssertEqual(model.detail?.album.id, "b")
        XCTAssertEqual(model.phase, .content)
    }

    func testRefreshFailureRetainsContent() async {
        let client = SuspendedAlbumClient()
        let model = MediaViewModel(client: client)
        await resolveInitial(model, client: client, albumID: "a")

        let refresh = Task { await model.refresh(album: album("a"), successMessage: "完成") }
        await client.waitForAlbumCalls(2)
        XCTAssertEqual(model.phase, .refreshing)
        await client.rejectAlbumCall(1)
        await refresh.value

        XCTAssertEqual(model.detail?.album.id, "a")
        XCTAssertNotNil(model.refreshError)
        XCTAssertEqual(model.phase, .content)
        XCTAssertNil(model.operationMessage)
    }

    func testRefreshAfterBatchReloadsAlbumAndPublishesMessage() async {
        let client = SuspendedAlbumClient()
        let model = MediaViewModel(client: client)
        await resolveInitial(model, client: client, albumID: "a")

        let refresh = Task {
            await model.refreshAfterBatch(album: album("a"), message: "批量操作完成")
        }
        await client.waitForAlbumCalls(2)
        await client.resolveAlbumCall(1, with: detail("a"))
        await refresh.value

        XCTAssertEqual(model.operationMessage, "批量操作完成")
    }

    func testRefreshAfterBatchFromOldAlbumCannotReplaceCurrentAlbum() async {
        let client = SuspendedAlbumClient()
        let model = MediaViewModel(client: client)
        await resolveInitial(model, client: client, albumID: "a")
        let loadB = Task { await model.load(album: album("b")) }
        await client.waitForAlbumCalls(2)
        await client.resolveAlbumCall(1, with: detail("b"))
        await loadB.value

        let staleRefresh = Task {
            await model.refreshAfterBatch(album: album("a"), message: "旧操作完成")
        }
        for _ in 0..<100 {
            await Task.yield()
        }
        let callCount = await client.albumCallCount()
        if callCount == 3 {
            await client.resolveAlbumCall(2, with: detail("a"))
        }
        await staleRefresh.value

        XCTAssertEqual(callCount, 2)
        XCTAssertEqual(model.currentAlbumID, "b")
        XCTAssertEqual(model.detail?.album.id, "b")
        XCTAssertNil(model.operationMessage)
    }

    func testDeferredManagementSuccessAfterAlbumSwitchDoesNotRequestOldAlbum() async {
        let client = SuspendedAlbumClient()
        let model = MediaViewModel(client: client)
        let gate = DeferredSuccessGate()
        await resolveInitial(model, client: client, albumID: "a")

        let deferredSuccess = Task {
            await gate.wait()
            await model.refreshAfterBatch(album: album("a"), message: "标签已更新")
        }

        let loadB = Task { await model.load(album: album("b")) }
        await client.waitForAlbumCalls(2)
        await client.resolveAlbumCall(1, with: detail("b"))
        await loadB.value
        await gate.open()
        await deferredSuccess.value
        let albumCallCount = await client.albumCallCount()

        XCTAssertEqual(albumCallCount, 2)
        XCTAssertEqual(model.currentAlbumID, "b")
        XCTAssertEqual(model.detail?.album.id, "b")
        XCTAssertNil(model.operationMessage)
    }

    func testDeleteFinishingAfterAlbumSwitchDoesNotRefreshOldAlbum() async {
        let client = SuspendedAlbumClient()
        let model = MediaViewModel(client: client)
        await resolveInitial(model, client: client, albumID: "a")

        let deletion = Task { await model.deleteTrack(id: "track-a", album: album("a")) }
        await client.waitForDeleteCalls(1)

        let loadB = Task { await model.load(album: album("b")) }
        await client.waitForAlbumCalls(2)
        await client.resolveAlbumCall(1, with: detail("b"))
        await loadB.value

        await client.resolveDeleteCall(0)
        let didRefresh = await deletion.value
        let albumCallCount = await client.albumCallCount()

        XCTAssertFalse(didRefresh)
        XCTAssertEqual(albumCallCount, 2)
        XCTAssertEqual(model.currentAlbumID, "b")
        XCTAssertEqual(model.detail?.album.id, "b")
        XCTAssertNil(model.operationMessage)
    }

    func testMakeBatchControllerCreatesAlbumUnboundController() {
        let model = MediaViewModel(client: SuspendedAlbumClient())

        let controller = model.makeBatchController()

        XCTAssertNil(controller.albumID)
        XCTAssertFalse(controller.isRunning)
    }

    func testRetryAfterInitialFailureClearsPreviousErrorWhileLoading() async {
        let client = SuspendedAlbumClient()
        let model = MediaViewModel(client: client)

        let failedLoad = Task { await model.load(album: album("a")) }
        await client.waitForAlbumCalls(1)
        await client.rejectAlbumCall(0)
        await failedLoad.value
        guard case .failed = model.phase else {
            return XCTFail("expected initial failure")
        }

        let retry = Task { await model.load(album: album("a")) }
        await client.waitForAlbumCalls(2)

        XCTAssertEqual(model.phase, .loading)
        XCTAssertNil(model.errorMessage)

        await client.resolveAlbumCall(1, with: detail("a"))
        await retry.value
    }

    func testCoverFailureLeavesPublishedDetailContent() async {
        let client = SuspendedAlbumClient()
        let model = MediaViewModel(client: client)
        let routedAlbum = album("a", coverArt: "cover-a")

        let load = Task { await model.load(album: routedAlbum) }
        await client.waitForAlbumCalls(1)
        await client.waitForCoverCalls(1)
        await client.resolveAlbumCall(0, with: detail("a", coverArt: "cover-a"))
        await waitUntil { model.phase == .content }

        XCTAssertEqual(model.detail?.album.id, "a", "detail must publish before cover resolution")
        await client.rejectCoverCall(0)
        await load.value

        XCTAssertEqual(model.phase, .content)
        XCTAssertEqual(model.detail?.album.id, "a")
        XCTAssertNotNil(model.coverError)
        XCTAssertNil(model.coverURL)
    }

    func testRefreshClearsOldCoverWhenFetchedCoverChangesBeforeNewURLResolves() async {
        let client = SuspendedAlbumClient()
        let model = MediaViewModel(client: client)
        let original = album("a", coverArt: "old-cover")
        await resolveInitial(model, client: client, album: original, coverURL: "https://example.test/old")

        let refresh = Task { await model.refresh(album: original, successMessage: "完成") }
        await client.waitForAlbumCalls(2)
        await client.waitForCoverCalls(2)
        await client.resolveAlbumCall(1, with: detail("a", coverArt: "new-cover"))
        await client.waitForCoverCalls(3)
        await waitUntil { model.detail?.album.coverArt == "new-cover" }

        XCTAssertNil(model.coverURL, "an old-cover URL must not be published for new-cover detail")
        XCTAssertNil(model.operationMessage)

        await client.resolveCoverCall(1, with: "https://example.test/stale")
        await client.resolveCoverCall(2, with: "https://example.test/new")
        await refresh.value
    }

    func testReplaceCoverPublishesSuccessOnlyAfterReloadedCoverResolves() async {
        let client = SuspendedAlbumClient()
        let model = MediaViewModel(client: client)
        let original = album("a", coverArt: "old-cover")
        await resolveInitial(model, client: client, album: original, coverURL: "https://example.test/old")

        let replacement = Task { await model.replaceCover(album: original, path: "/tmp/new.jpg") }
        await client.waitForAlbumCalls(2)
        await client.waitForCoverCalls(2)
        await client.resolveAlbumCall(1, with: detail("a", coverArt: "new-cover"))
        await client.waitForCoverCalls(3)
        await waitUntil { model.detail?.album.coverArt == "new-cover" }

        XCTAssertNil(model.operationMessage)
        XCTAssertEqual(model.coverRevision, 0)

        await client.resolveCoverCall(1, with: "https://example.test/stale")
        await client.resolveCoverCall(2, with: "https://example.test/new")
        await replacement.value

        let recordedReplacement = await client.recordedCoverReplacement()
        XCTAssertEqual(recordedReplacement, .init(albumID: "a", path: "/tmp/new.jpg"))
        XCTAssertEqual(model.coverURL?.absoluteString, "https://example.test/new")
        XCTAssertEqual(model.coverRevision, 1)
        XCTAssertEqual(model.operationMessage, "封面已更新")
        XCTAssertNil(model.operationError)
    }

    func testPermissionErrorsUseReauthenticationMessage() {
        XCTAssertEqual(
            LibraryOperationErrorPresentation.message(CoreError.NotAuthenticated),
            "权限已变化，请重新登录"
        )
        XCTAssertEqual(
            LibraryOperationErrorPresentation.message(CoreError.Server(code: 50, message: "not authorized")),
            "权限已变化，请重新登录"
        )
    }

    func testOrdinaryNetworkErrorRetainsLocalizedMessageWithoutAuthenticatedURL() {
        let error = CoreError.Network(message: "网络不可用")
        let message = LibraryOperationErrorPresentation.message(error)

        XCTAssertEqual(message, error.localizedDescription)
        XCTAssertFalse(message.contains("token="))
    }

    func testAuthenticatedURLIsRedactedFromOrdinaryError() {
        let error = CoreError.Network(message: "GET https://music.test/rest/getCoverArt?u=me&t=secret failed")
        let message = LibraryOperationErrorPresentation.message(error)

        XCTAssertFalse(message.contains("https://music.test"))
        XCTAssertFalse(message.contains("secret"))
    }

    func testUppercaseAuthenticatedURLIsRedactedFromOrdinaryError() {
        let error = CoreError.Network(message: "GET HTTPS://music.test/rest/getCoverArt?u=me&t=secret failed")
        let message = LibraryOperationErrorPresentation.message(error)

        XCTAssertFalse(message.contains("HTTPS://music.test"))
        XCTAssertFalse(message.contains("secret"))
    }
}

@MainActor
private func resolveInitial(
    _ model: MediaViewModel,
    client: SuspendedAlbumClient,
    album: Album,
    coverURL: String
) async {
    let load = Task { await model.load(album: album) }
    await client.waitForAlbumCalls(1)
    await client.waitForCoverCalls(1)
    await client.resolveAlbumCall(0, with: AlbumDetail(album: album, tracks: []))
    await client.resolveCoverCall(0, with: coverURL)
    await load.value
}

@MainActor
private func resolveInitial(
    _ model: MediaViewModel,
    client: SuspendedAlbumClient,
    albumID: String
) async {
    let load = Task { await model.load(album: album(albumID)) }
    await client.waitForAlbumCalls(1)
    await client.resolveAlbumCall(0, with: detail(albumID))
    await load.value
}

@MainActor
private func waitUntil(_ condition: @escaping @MainActor () -> Bool) async {
    for _ in 0..<100 where !condition() {
        await Task.yield()
    }
    XCTAssertTrue(condition())
}

private func album(_ id: String, coverArt: String? = nil) -> Album {
    Album(
        id: id, name: "Album \(id)", artist: "Artist", artistId: "artist", coverArt: coverArt,
        songCount: 0, duration: 0, year: nil, genre: nil, created: nil
    )
}

private func detail(_ id: String, coverArt: String? = nil) -> AlbumDetail {
    AlbumDetail(album: album(id, coverArt: coverArt), tracks: [])
}

private enum TestFailure: LocalizedError {
    case rejected

    var errorDescription: String? { "request rejected" }
}

private actor DeferredSuccessGate {
    private var opened = false
    private var continuation: CheckedContinuation<Void, Never>?

    func wait() async {
        guard !opened else { return }
        await withCheckedContinuation { continuation = $0 }
    }

    func open() {
        opened = true
        continuation?.resume()
        continuation = nil
    }
}

private actor SuspendedAlbumClient: MusicClientProviding {
    struct CoverReplacement: Equatable, Sendable {
        let albumID: String
        let path: String
    }

    private var albumCalls: [CheckedContinuation<AlbumDetail, Error>?] = []
    private var coverCalls: [CheckedContinuation<String, Error>?] = []
    private var albumWaiters: [(Int, CheckedContinuation<Void, Never>)] = []
    private var coverWaiters: [(Int, CheckedContinuation<Void, Never>)] = []
    private var deleteCalls: [CheckedContinuation<Void, Error>?] = []
    private var deleteWaiters: [(Int, CheckedContinuation<Void, Never>)] = []
    private(set) var coverReplacement: CoverReplacement?

    func getAlbum(id: String) async throws -> AlbumDetail {
        try await withCheckedThrowingContinuation { continuation in
            albumCalls.append(continuation)
            resumeSatisfiedWaiters()
        }
    }

    func coverArtURL(id: String, size: UInt32?) async throws -> String {
        try await withCheckedThrowingContinuation { continuation in
            coverCalls.append(continuation)
            resumeSatisfiedWaiters()
        }
    }

    func setCoverArt(albumID: String, localPath: String) async throws {
        coverReplacement = .init(albumID: albumID, path: localPath)
    }

    func waitForAlbumCalls(_ count: Int) async {
        guard albumCalls.count < count else { return }
        await withCheckedContinuation { albumWaiters.append((count, $0)) }
    }

    func waitForCoverCalls(_ count: Int) async {
        guard coverCalls.count < count else { return }
        await withCheckedContinuation { coverWaiters.append((count, $0)) }
    }

    func waitForDeleteCalls(_ count: Int) async {
        guard deleteCalls.count < count else { return }
        await withCheckedContinuation { deleteWaiters.append((count, $0)) }
    }

    func resolveAlbumCall(_ index: Int, with value: AlbumDetail) {
        albumCalls[index]?.resume(returning: value)
        albumCalls[index] = nil
    }

    func rejectAlbumCall(_ index: Int) {
        albumCalls[index]?.resume(throwing: TestFailure.rejected)
        albumCalls[index] = nil
    }

    func resolveCoverCall(_ index: Int, with value: String) {
        coverCalls[index]?.resume(returning: value)
        coverCalls[index] = nil
    }

    func rejectCoverCall(_ index: Int) {
        coverCalls[index]?.resume(throwing: TestFailure.rejected)
        coverCalls[index] = nil
    }

    func resolveDeleteCall(_ index: Int) {
        deleteCalls[index]?.resume(returning: ())
        deleteCalls[index] = nil
    }

    func recordedCoverReplacement() -> CoverReplacement? { coverReplacement }
    func albumCallCount() -> Int { albumCalls.count }

    private func resumeSatisfiedWaiters() {
        let readyAlbums = albumWaiters.filter { $0.0 <= albumCalls.count }
        albumWaiters.removeAll { $0.0 <= albumCalls.count }
        readyAlbums.forEach { $0.1.resume() }
        let readyCovers = coverWaiters.filter { $0.0 <= coverCalls.count }
        coverWaiters.removeAll { $0.0 <= coverCalls.count }
        readyCovers.forEach { $0.1.resume() }
        let readyDeletes = deleteWaiters.filter { $0.0 <= deleteCalls.count }
        deleteWaiters.removeAll { $0.0 <= deleteCalls.count }
        readyDeletes.forEach { $0.1.resume() }
    }

    func login(server: String, user: String, password: String) async throws -> SessionValue {
        .init(server: server, user: user)
    }
    func logout() async {}
    func listAlbums(offset: UInt32, size: UInt32) async throws -> [Album] { [] }
    func search(query: String) async throws -> SearchResult { .init(artists: [], albums: [], tracks: []) }
    func upload(localPath: String, libraryKey: String, progress: UploadProgress) async throws -> Track {
        throw CocoaError(.featureUnsupported)
    }
    func updateTags(id: String, update: TagUpdate) async throws {}
    func deleteTrack(id: String) async throws {
        try await withCheckedThrowingContinuation { continuation in
            deleteCalls.append(continuation)
            resumeSatisfiedWaiters()
        }
    }
    func moveTrack(id: String, key: String) async throws {}
    func startScan() async throws -> ScanStatus { throw CocoaError(.featureUnsupported) }
    func scanStatus() async throws -> ScanStatus { throw CocoaError(.featureUnsupported) }
}
