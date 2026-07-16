import XCTest
import YevuneCoreFFI
@testable import Yevune

@MainActor
final class LibraryBrowseViewModelTests: XCTestCase {
    func testInitialAlbumRequestUsesZeroOffsetAndSixtyItems() async {
        let client = SuspendedLibraryClient()
        let model = LibraryBrowseViewModel(client: client)

        let reload = Task { await model.reload() }
        await client.waitForAlbumCallCount(1)

        let call = await client.albumCall(at: 0)
        XCTAssertEqual(call.offset, 0)
        XCTAssertEqual(call.size, 60)
        assertSort(call.filter, equals: .newest)

        await client.resolveAlbumCall(0, with: [album(0)])
        await reload.value
        XCTAssertEqual(model.albums.map(\.id), ["album-0"])
    }

    func testSixtyAlbumsRequestNextPageAtOffsetSixty() async {
        let client = SuspendedLibraryClient()
        let model = LibraryBrowseViewModel(client: client)
        await resolveReload(model, client: client, with: albums(0 ..< 60))

        let nextPage = Task { await model.loadNextPage() }
        await client.waitForAlbumCallCount(2)

        let call = await client.albumCall(at: 1)
        XCTAssertEqual(call.offset, 60)
        XCTAssertEqual(call.size, 60)
        await client.resolveAlbumCall(1, with: [album(60)])
        await nextPage.value
    }

    func testThirdPageCanAdvancePastOneHundredAlbums() async {
        let client = SuspendedLibraryClient()
        let model = LibraryBrowseViewModel(client: client)
        await resolveReload(model, client: client, with: albums(0 ..< 60))
        await resolveNextPage(model, client: client, callIndex: 1, with: albums(60 ..< 120))

        let thirdPage = Task { await model.loadNextPage() }
        await client.waitForAlbumCallCount(3)

        let thirdCall = await client.albumCall(at: 2)
        XCTAssertEqual(thirdCall.offset, 120)
        await client.resolveAlbumCall(2, with: [])
        await thirdPage.value
    }

    func testDuplicateIDsAreRemovedWithoutReorderingFirstOccurrences() async {
        let client = SuspendedLibraryClient()
        let model = LibraryBrowseViewModel(client: client)
        await resolveReload(model, client: client, with: albums(0 ..< 60))
        let page = [album(59)] + albums(60 ..< 119)

        await resolveNextPage(model, client: client, callIndex: 1, with: page)

        XCTAssertEqual(model.albums.map(\.id), (0 ..< 119).map { "album-\($0)" })
    }

    func testConcurrentNextPageRequestsAreSingleFlight() async {
        let client = SuspendedLibraryClient()
        let model = LibraryBrowseViewModel(client: client)
        await resolveReload(model, client: client, with: albums(0 ..< 60))

        let first = Task { await model.loadNextPage() }
        await client.waitForAlbumCallCount(2)
        let second = Task { await model.loadNextPage() }
        await Task.yield()

        let callCount = await client.albumCallCount()
        XCTAssertEqual(callCount, 2)
        await client.resolveAlbumCall(1, with: [])
        await first.value
        await second.value
    }

    func testNextPageDoesNotStartWhileReloadIsInFlight() async {
        let client = SuspendedLibraryClient()
        let model = LibraryBrowseViewModel(client: client)
        await resolveReload(model, client: client, with: albums(0 ..< 60))

        let refresh = Task { await model.reload() }
        await client.waitForAlbumCallCount(2)
        let nextPageStarted = expectation(description: "next page call started")
        let nextPage = Task {
            nextPageStarted.fulfill()
            await model.loadNextPage()
        }
        await fulfillment(of: [nextPageStarted], timeout: 1)
        await Task.yield()

        let callCount = await client.albumCallCount()
        XCTAssertEqual(callCount, 2)

        if callCount > 2 {
            await client.resolveAlbumCall(2, with: [])
        }
        await client.resolveAlbumCall(1, with: albums(0 ..< 60))
        await refresh.value
        await nextPage.value
    }

    func testExactFullPageAllowsEmptyTailBeforeClosingPagination() async {
        let client = SuspendedLibraryClient()
        let model = LibraryBrowseViewModel(client: client)
        await resolveReload(model, client: client, with: albums(0 ..< 60))
        XCTAssertTrue(model.hasMoreAlbums)

        await resolveNextPage(model, client: client, callIndex: 1, with: [])

        XCTAssertFalse(model.hasMoreAlbums)
        XCTAssertEqual(model.albums.count, 60)
    }

    func testGenreYearAndSortCriteriaCompletelyReplaceTheFilter() async {
        let client = SuspendedLibraryClient()
        let model = LibraryBrowseViewModel(client: client)

        model.selectCriterion(.genre("Jazz"))
        await client.waitForAlbumCallCount(1)
        assertGenre(await client.albumCall(at: 0).filter, equals: "Jazz")
        await client.resolveAlbumCall(0, with: [])
        await waitUntil { !model.isRefreshing }

        model.selectCriterion(.yearRange(from: 1990, to: 1999))
        await client.waitForAlbumCallCount(2)
        assertYearRange(await client.albumCall(at: 1).filter, from: 1990, to: 1999)
        await client.resolveAlbumCall(1, with: [])
        await waitUntil { !model.isRefreshing }

        model.selectCriterion(.sort(.alphabeticalByName))
        await client.waitForAlbumCallCount(3)
        assertSort(await client.albumCall(at: 2).filter, equals: .alphabeticalByName)
        await client.resolveAlbumCall(2, with: [])
        await waitUntil { !model.isRefreshing }
    }

    func testInvalidYearRangePublishesValidationAndMakesNoRequest() async {
        let client = SuspendedLibraryClient()
        let model = LibraryBrowseViewModel(client: client)

        model.selectCriterion(.yearRange(from: 2025, to: 2024))
        await Task.yield()

        XCTAssertNotNil(model.validationMessage)
        let callCount = await client.albumCallCount()
        XCTAssertEqual(callCount, 0)
        XCTAssertFalse(model.isRefreshing)
    }

    func testInitialFailureUsesInitialError() async {
        let client = SuspendedLibraryClient()
        let model = LibraryBrowseViewModel(client: client)
        let reload = Task { await model.reload() }
        await client.waitForAlbumCallCount(1)

        await client.rejectAlbumCall(0)
        await reload.value

        XCTAssertNotNil(model.initialError)
        XCTAssertNil(model.refreshError)
        XCTAssertTrue(model.albums.isEmpty)
    }

    func testRefreshFailureRetainsExistingAlbumsAndUsesRefreshError() async {
        let client = SuspendedLibraryClient()
        let model = LibraryBrowseViewModel(client: client)
        await resolveReload(model, client: client, with: [album(1)])

        let refresh = Task { await model.reload() }
        await client.waitForAlbumCallCount(2)
        await client.rejectAlbumCall(1)
        await refresh.value

        XCTAssertEqual(model.albums.map(\.id), ["album-1"])
        XCTAssertNotNil(model.refreshError)
        XCTAssertNil(model.initialError)
    }

    func testNextPageFailureRetainsAlbumsAndUsesNextPageError() async {
        let client = SuspendedLibraryClient()
        let model = LibraryBrowseViewModel(client: client)
        await resolveReload(model, client: client, with: albums(0 ..< 60))

        let nextPage = Task { await model.loadNextPage() }
        await client.waitForAlbumCallCount(2)
        await client.rejectAlbumCall(1)
        await nextPage.value

        XCTAssertEqual(model.albums.count, 60)
        XCTAssertNotNil(model.nextPageError)
        XCTAssertNil(model.initialError)
        XCTAssertNil(model.refreshError)
    }

    func testLateResponseCannotOverwriteNewCriterion() async {
        let client = SuspendedLibraryClient()
        let model = LibraryBrowseViewModel(client: client)

        model.selectCriterion(.genre("Old"))
        await client.waitForAlbumCallCount(1)
        model.selectCriterion(.genre("New"))
        await client.waitForAlbumCallCount(2)

        await client.rejectAlbumCall(0)
        await Task.yield()

        XCTAssertTrue(model.albums.isEmpty)
        XCTAssertNil(model.initialError)
        XCTAssertNil(model.refreshError)
        XCTAssertTrue(model.isRefreshing)

        await client.resolveAlbumCall(1, with: [album(2)])
        await waitUntil { !model.isRefreshing }

        XCTAssertEqual(model.albums.map(\.id), ["album-2"])
        XCTAssertNil(model.initialError)
        XCTAssertNil(model.refreshError)
        XCTAssertFalse(model.isRefreshing)
        assertGenre(model.albumCriterion.filter, equals: "New")
    }

    func testLateSuccessfulResponseCannotReplaceNewCriterionContent() async {
        let client = SuspendedLibraryClient()
        let model = LibraryBrowseViewModel(client: client)

        model.selectCriterion(.genre("Old"))
        await client.waitForAlbumCallCount(1)
        model.selectCriterion(.genre("New"))
        await client.waitForAlbumCallCount(2)
        await client.resolveAlbumCall(1, with: [album(2)])
        await waitUntil { !model.isRefreshing }

        await client.resolveAlbumCall(0, with: [album(1)])
        await Task.yield()

        XCTAssertEqual(model.albums.map(\.id), ["album-2"])
        XCTAssertNil(model.initialError)
        XCTAssertNil(model.refreshError)
        XCTAssertFalse(model.isRefreshing)
    }

    func testArtistsUseSortNameThenNameForLocalizedStandardOrdering() async {
        let client = SuspendedLibraryClient(artists: [
            artist(id: "z", name: "Zulu", sortName: "2 Beta"),
            artist(id: "a", name: "Alpha", sortName: nil),
            artist(id: "t", name: "Ten", sortName: "10 Gamma"),
        ])
        let model = LibraryBrowseViewModel(client: client)

        model.selectSection(.artists)
        await client.waitForArtistCallCount(1)
        await waitUntil { !model.isRefreshing }

        XCTAssertEqual(model.artists.map(\.id), ["z", "t", "a"])
        let artistCallCount = await client.artistCallCount()
        XCTAssertEqual(artistCallCount, 1)
    }

    func testSelectingCurrentSectionOrCriterionDoesNotReload() async {
        let client = SuspendedLibraryClient()
        let model = LibraryBrowseViewModel(client: client)

        model.selectSection(.albums)
        model.selectCriterion(.sort(.newest))
        await Task.yield()

        let callCount = await client.albumCallCount()
        XCTAssertEqual(callCount, 0)
    }

    private func resolveReload(
        _ model: LibraryBrowseViewModel,
        client: SuspendedLibraryClient,
        with response: [Album]
    ) async {
        let reload = Task { await model.reload() }
        let index = await client.albumCallCount()
        await client.waitForAlbumCallCount(index + 1)
        await client.resolveAlbumCall(index, with: response)
        await reload.value
    }

    private func resolveNextPage(
        _ model: LibraryBrowseViewModel,
        client: SuspendedLibraryClient,
        callIndex: Int,
        with response: [Album]
    ) async {
        let nextPage = Task { await model.loadNextPage() }
        await client.waitForAlbumCallCount(callIndex + 1)
        await client.resolveAlbumCall(callIndex, with: response)
        await nextPage.value
    }

    private func assertSort(
        _ filter: AlbumFilter?,
        equals expected: AlbumSort,
        file: StaticString = #filePath,
        line: UInt = #line
    ) {
        guard case .sort(let actual) = filter else {
            return XCTFail("Expected sort filter, got \(String(describing: filter))", file: file, line: line)
        }
        XCTAssertEqual(actual, expected, file: file, line: line)
    }

    private func assertGenre(
        _ filter: AlbumFilter?,
        equals expected: String,
        file: StaticString = #filePath,
        line: UInt = #line
    ) {
        guard case .genre(let actual) = filter else {
            return XCTFail("Expected genre filter, got \(String(describing: filter))", file: file, line: line)
        }
        XCTAssertEqual(actual, expected, file: file, line: line)
    }

    private func assertYearRange(
        _ filter: AlbumFilter?,
        from expectedFrom: UInt32,
        to expectedTo: UInt32,
        file: StaticString = #filePath,
        line: UInt = #line
    ) {
        guard case .yearRange(let actualFrom, let actualTo) = filter else {
            return XCTFail("Expected year range filter, got \(String(describing: filter))", file: file, line: line)
        }
        XCTAssertEqual(actualFrom, expectedFrom, file: file, line: line)
        XCTAssertEqual(actualTo, expectedTo, file: file, line: line)
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

private actor SuspendedLibraryClient: MusicClientProviding {
    struct AlbumCall: Sendable {
        let filter: AlbumFilter
        let offset: UInt32
        let size: UInt32
    }

    private let artistResponse: [Artist]
    private var albumCalls: [AlbumCall] = []
    private var albumWaiters: [CheckedContinuation<[Album], Error>] = []
    private var callCountWaiters: [(count: Int, continuation: CheckedContinuation<Void, Never>)] = []
    private var artistCallCountWaiters: [(count: Int, continuation: CheckedContinuation<Void, Never>)] = []
    private var artistCalls = 0

    init(artists: [Artist] = []) {
        artistResponse = artists
    }

    func listAlbums(offset: UInt32, size: UInt32) async throws -> [Album] {
        try await listAlbums(filter: .sort(.newest), offset: offset, size: size)
    }

    func listAlbums(filter: AlbumFilter, offset: UInt32, size: UInt32) async throws -> [Album] {
        albumCalls.append(.init(filter: filter, offset: offset, size: size))
        resumeSatisfiedCallCountWaiters()
        return try await withCheckedThrowingContinuation { albumWaiters.append($0) }
    }

    func listGenres() async throws -> [Genre] {
        [Genre(value: "Jazz", songCount: 1, albumCount: 1)]
    }

    func listArtists() async throws -> [Artist] {
        artistCalls += 1
        resumeSatisfiedArtistCallCountWaiters()
        return artistResponse
    }

    func login(server: String, user: String, password: String) async throws -> SessionValue {
        throw BrowseTestError.failed
    }

    func logout() async {}

    func search(query: String) async throws -> SearchResult { throw BrowseTestError.failed }

    func upload(localPath: String, libraryKey: String, progress: UploadProgress) async throws -> Track {
        throw BrowseTestError.failed
    }

    func updateTags(id: String, update: TagUpdate) async throws { throw BrowseTestError.failed }
    func deleteTrack(id: String) async throws { throw BrowseTestError.failed }
    func moveTrack(id: String, key: String) async throws { throw BrowseTestError.failed }
    func startScan() async throws -> ScanStatus { throw BrowseTestError.failed }
    func scanStatus() async throws -> ScanStatus { throw BrowseTestError.failed }

    func albumCallCount() -> Int { albumCalls.count }
    func albumCall(at index: Int) -> AlbumCall { albumCalls[index] }
    func artistCallCount() -> Int { artistCalls }

    func waitForAlbumCallCount(_ count: Int) async {
        guard albumCalls.count < count else { return }
        await withCheckedContinuation { continuation in
            callCountWaiters.append((count, continuation))
        }
    }

    func waitForArtistCallCount(_ count: Int) async {
        guard artistCalls < count else { return }
        await withCheckedContinuation { continuation in
            artistCallCountWaiters.append((count, continuation))
        }
    }

    func resolveAlbumCall(_ index: Int, with albums: [Album]) {
        albumWaiters[index].resume(returning: albums)
    }

    func rejectAlbumCall(_ index: Int) {
        albumWaiters[index].resume(throwing: BrowseTestError.failed)
    }

    private func resumeSatisfiedCallCountWaiters() {
        let ready = callCountWaiters.filter { $0.count <= albumCalls.count }
        callCountWaiters.removeAll { $0.count <= albumCalls.count }
        ready.forEach { $0.continuation.resume() }
    }

    private func resumeSatisfiedArtistCallCountWaiters() {
        let ready = artistCallCountWaiters.filter { $0.count <= artistCalls }
        artistCallCountWaiters.removeAll { $0.count <= artistCalls }
        ready.forEach { $0.continuation.resume() }
    }
}

private enum BrowseTestError: LocalizedError {
    case failed
    var errorDescription: String? { "browse failed" }
}

private func album(_ index: Int) -> Album {
    Album(
        id: "album-\(index)",
        name: "Album \(index)",
        artist: nil,
        artistId: nil,
        coverArt: nil,
        songCount: 1,
        duration: 1,
        year: nil,
        genre: nil,
        created: nil
    )
}

private func albums(_ range: Range<Int>) -> [Album] {
    range.map(album)
}

private func artist(id: String, name: String, sortName: String?) -> Artist {
    Artist(
        id: id,
        name: name,
        sortName: sortName,
        coverArt: nil,
        musicBrainzId: nil,
        albumCount: 0
    )
}
