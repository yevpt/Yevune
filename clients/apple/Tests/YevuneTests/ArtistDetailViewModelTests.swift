import XCTest
import YevuneCoreFFI
@testable import Yevune

@MainActor
final class ArtistDetailViewModelTests: XCTestCase {
    func testSupersededLoadCannotStartAfterNewGeneration() async {
        let client = SuspendedArtistClient()
        let model = ArtistDetailViewModel(client: client)

        model.load(artistID: "artist-a")
        model.load(artistID: "artist-b")
        await client.waitForCallCount(1)

        let callIDs = await client.callIDs()
        XCTAssertEqual(callIDs, ["artist-b"])

        await client.resolveCall(0, with: detail(id: "artist-b"))
        await waitUntil { !model.isLoading }
    }

    func testLateArtistSuccessCannotOverwriteNewArtist() async {
        let client = SuspendedArtistClient()
        let model = ArtistDetailViewModel(client: client)

        model.load(artistID: "artist-a")
        await client.waitForCallCount(1)
        model.load(artistID: "artist-b")
        await client.waitForCallCount(2)

        await client.resolveCall(1, with: detail(id: "artist-b"))
        await waitUntil { !model.isLoading }
        await client.resolveCall(0, with: detail(id: "artist-a"))
        await Task.yield()

        XCTAssertEqual(model.detail?.artist.id, "artist-b")
        XCTAssertFalse(model.isLoading)
        XCTAssertNil(model.errorMessage)
    }

    func testLateArtistFailureCannotStopOrFailNewArtistLoad() async {
        let client = SuspendedArtistClient()
        let model = ArtistDetailViewModel(client: client)

        model.load(artistID: "artist-a")
        await client.waitForCallCount(1)
        model.load(artistID: "artist-b")
        await client.waitForCallCount(2)

        await client.rejectCall(0)
        await Task.yield()

        XCTAssertTrue(model.isLoading)
        XCTAssertNil(model.errorMessage)
        XCTAssertNil(model.detail)

        await client.resolveCall(1, with: detail(id: "artist-b"))
        await waitUntil { !model.isLoading }

        XCTAssertEqual(model.detail?.artist.id, "artist-b")
        XCTAssertNil(model.errorMessage)
    }

    private func waitUntil(
        _ condition: @MainActor () -> Bool,
        file: StaticString = #filePath,
        line: UInt = #line
    ) async {
        for _ in 0 ..< 1_000 where !condition() {
            await Task.yield()
        }
        XCTAssertTrue(condition(), "Condition did not become true", file: file, line: line)
    }
}

private actor SuspendedArtistClient: MusicClientProviding {
    private var calls: [String] = []
    private var continuations: [CheckedContinuation<ArtistDetail, Error>] = []
    private var callCountWaiters: [(Int, CheckedContinuation<Void, Never>)] = []

    func getArtist(id: String) async throws -> ArtistDetail {
        calls.append(id)
        resumeSatisfiedWaiters()
        return try await withCheckedThrowingContinuation { continuations.append($0) }
    }

    func waitForCallCount(_ count: Int) async {
        guard calls.count < count else { return }
        await withCheckedContinuation { callCountWaiters.append((count, $0)) }
    }

    func callIDs() -> [String] {
        calls
    }

    func resolveCall(_ index: Int, with detail: ArtistDetail) {
        continuations[index].resume(returning: detail)
    }

    func rejectCall(_ index: Int) {
        continuations[index].resume(throwing: ArtistTestError.failed)
    }

    private func resumeSatisfiedWaiters() {
        let satisfied = callCountWaiters.filter { calls.count >= $0.0 }
        callCountWaiters.removeAll { calls.count >= $0.0 }
        satisfied.forEach { $0.1.resume() }
    }

    func login(server: String, user: String, password: String) async throws -> SessionValue {
        throw ArtistTestError.failed
    }

    func logout() async {}
    func listAlbums(offset: UInt32, size: UInt32) async throws -> [Album] { throw ArtistTestError.failed }
    func search(query: String) async throws -> SearchResult { throw ArtistTestError.failed }
    func upload(localPath: String, libraryKey: String, progress: UploadProgress) async throws -> Track {
        throw ArtistTestError.failed
    }
    func updateTags(id: String, update: TagUpdate) async throws { throw ArtistTestError.failed }
    func deleteTrack(id: String) async throws { throw ArtistTestError.failed }
    func moveTrack(id: String, key: String) async throws { throw ArtistTestError.failed }
    func startScan() async throws -> ScanStatus { throw ArtistTestError.failed }
    func scanStatus() async throws -> ScanStatus { throw ArtistTestError.failed }
}

private enum ArtistTestError: Error {
    case failed
}

private func detail(id: String) -> ArtistDetail {
    ArtistDetail(
        artist: Artist(
            id: id,
            name: id,
            sortName: nil,
            coverArt: nil,
            musicBrainzId: nil,
            albumCount: 0
        ),
        albums: []
    )
}
