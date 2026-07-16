import XCTest
import YevuneCoreFFI
@testable import Yevune

@MainActor
final class LibrarySearchViewModelTests: XCTestCase {
    func testInputWaitsForInjectedDebounceBeforeRequesting() async {
        let client = SuspendedSearchClient()
        let sleeper = ControlledSearchSleeper()
        let controlledSleep: SearchSleeper = { duration in
            try await sleeper.callAsFunction(duration)
        }
        let model = LibrarySearchViewModel(client: client, sleeper: controlledSleep)

        model.setInput("needle")
        await sleeper.waitForCallCount(1)

        XCTAssertEqual(model.phase, .debouncing)
        let callCount = await client.callCount()
        let duration = await sleeper.duration(at: 0)
        XCTAssertEqual(callCount, 0)
        XCTAssertEqual(duration, .milliseconds(250))

        await sleeper.resolveCall(0)
        await client.waitForCallCount(1)

        XCTAssertEqual(model.phase, .loading)
        await client.resolveCall(0, with: page())
        await waitUntil { model.phase == .empty }
    }

    func testInitialRequestTrimsQueryAndRequestsTwentyFourOfEveryCategory() async {
        let client = SuspendedSearchClient()
        let model = makeImmediateModel(client: client)

        model.setInput("  moon  ")
        await client.waitForCallCount(1)

        let request = await client.request(at: 0)
        XCTAssertEqual(model.input, "  moon  ")
        XCTAssertEqual(model.query, "moon")
        XCTAssertEqual(request.query, "moon")
        XCTAssertEqual(request.artistOffset, 0)
        XCTAssertEqual(request.artistCount, 24)
        XCTAssertEqual(request.albumOffset, 0)
        XCTAssertEqual(request.albumCount, 24)
        XCTAssertEqual(request.trackOffset, 0)
        XCTAssertEqual(request.trackCount, 24)

        await client.resolveCall(0, with: page())
        await waitUntil { model.phase == .empty }
    }

    func testWhitespaceInputImmediatelyClearsResultsAndReturnsToIdle() async {
        let client = SuspendedSearchClient()
        let model = makeImmediateModel(client: client)
        model.setInput("filled")
        await client.waitForCallCount(1)
        await client.resolveCall(0, with: page(
            artists: [artist("artist")],
            albums: [album("album")],
            tracks: [track("track")]
        ))
        await waitUntil { model.phase == .results }

        model.setInput(" \n\t ")

        XCTAssertEqual(model.input, "")
        XCTAssertEqual(model.query, "")
        XCTAssertEqual(model.phase, .idle)
        XCTAssertTrue(model.artists.isEmpty)
        XCTAssertTrue(model.albums.isEmpty)
        XCTAssertTrue(model.tracks.isEmpty)
        XCTAssertFalse(model.hasMoreArtists)
        XCTAssertFalse(model.hasMoreAlbums)
        XCTAssertFalse(model.hasMoreTracks)
        XCTAssertTrue(model.nextPageErrors.isEmpty)
    }

    func testLateQueryCannotOverwriteCurrentQuery() async {
        let client = SuspendedSearchClient()
        let model = makeImmediateModel(client: client)

        model.setInput("A")
        await client.waitForCallCount(1)
        model.setInput("B")
        await client.waitForCallCount(2)

        await client.resolveCall(1, with: page(albums: [album("B")]))
        await waitUntil { model.albums.map(\.id) == ["album-B"] }
        await client.resolveCall(0, with: page(albums: [album("A")]))
        await Task.yield()

        XCTAssertEqual(model.input, "B")
        XCTAssertEqual(model.query, "B")
        XCTAssertEqual(model.phase, .results)
        XCTAssertEqual(model.albums.map(\.id), ["album-B"])
    }

    func testClearPreventsUncooperativeOldResponseFromWritingBack() async {
        let client = SuspendedSearchClient()
        let model = makeImmediateModel(client: client)

        model.setInput("old")
        await client.waitForCallCount(1)
        model.clear()
        await client.resolveCall(0, with: page(tracks: [track("old")]))
        await Task.yield()

        XCTAssertEqual(model.phase, .idle)
        XCTAssertEqual(model.input, "")
        XCTAssertEqual(model.query, "")
        XCTAssertTrue(model.tracks.isEmpty)
    }

    func testInitialResultsStablyDeduplicateAllThreeCategories() async {
        let client = SuspendedSearchClient()
        let model = makeImmediateModel(client: client)

        model.setInput("duplicates")
        await client.waitForCallCount(1)
        await client.resolveCall(0, with: page(
            artists: [artist("1"), artist("1"), artist("2")],
            albums: [album("1"), album("1"), album("2")],
            tracks: [track("1"), track("1"), track("2")]
        ))
        await waitUntil { model.phase == .results }

        XCTAssertEqual(model.artists.map(\.id), ["artist-1", "artist-2"])
        XCTAssertEqual(model.albums.map(\.id), ["album-1", "album-2"])
        XCTAssertEqual(model.tracks.map(\.id), ["track-1", "track-2"])
    }

    func testEachCategoryPaginationRequestsOnlyItsOwnCountAndCurrentOffset() async {
        let client = SuspendedSearchClient()
        let model = makeImmediateModel(client: client)
        model.setInput("pages")
        await client.waitForCallCount(1)
        await client.resolveCall(0, with: page(
            artists: [artist("1")],
            albums: [album("1"), album("2")],
            tracks: [track("1"), track("2"), track("3")],
            hasMoreArtists: true,
            hasMoreAlbums: true,
            hasMoreTracks: true
        ))
        await waitUntil { model.phase == .results }

        let artistLoad = Task { await model.loadMore(.artists) }
        await client.waitForCallCount(2)
        let artistRequest = await client.request(at: 1)
        XCTAssertEqual(artistRequest, request(
            query: "pages", artistOffset: 1, artistCount: 24
        ))
        await client.resolveCall(1, with: page())
        await artistLoad.value

        let albumLoad = Task { await model.loadMore(.albums) }
        await client.waitForCallCount(3)
        let albumRequest = await client.request(at: 2)
        XCTAssertEqual(albumRequest, request(
            query: "pages", albumOffset: 2, albumCount: 24
        ))
        await client.resolveCall(2, with: page())
        await albumLoad.value

        let trackLoad = Task { await model.loadMore(.tracks) }
        await client.waitForCallCount(4)
        let trackRequest = await client.request(at: 3)
        XCTAssertEqual(trackRequest, request(
            query: "pages", trackOffset: 3, trackCount: 24
        ))
        await client.resolveCall(3, with: page())
        await trackLoad.value
    }

    func testAlbumPaginationAppendsUniqueAlbumsWithoutReplacingOtherCategories() async {
        let client = SuspendedSearchClient()
        let model = makeImmediateModel(client: client)
        model.setInput("append")
        await client.waitForCallCount(1)
        await client.resolveCall(0, with: page(
            artists: [artist("kept")],
            albums: [album("1"), album("2")],
            tracks: [track("kept")],
            hasMoreAlbums: true
        ))
        await waitUntil { model.phase == .results }

        let load = Task { await model.loadMore(.albums) }
        await client.waitForCallCount(2)
        await client.resolveCall(1, with: page(
            artists: [artist("ignored")],
            albums: [album("2"), album("3"), album("3"), album("4")],
            tracks: [track("ignored")],
            hasMoreAlbums: false
        ))
        await load.value

        XCTAssertEqual(model.artists.map(\.id), ["artist-kept"])
        XCTAssertEqual(model.albums.map(\.id), ["album-1", "album-2", "album-3", "album-4"])
        XCTAssertEqual(model.tracks.map(\.id), ["track-kept"])
        XCTAssertFalse(model.hasMoreAlbums)
    }

    func testNextPageFailureOnlySetsTargetCategoryErrorAndRetainsResults() async {
        let client = SuspendedSearchClient()
        let model = makeImmediateModel(client: client)
        model.setInput("failure")
        await client.waitForCallCount(1)
        await client.resolveCall(0, with: page(
            artists: [artist("kept")],
            albums: [album("kept")],
            tracks: [track("kept")],
            hasMoreAlbums: true
        ))
        await waitUntil { model.phase == .results }

        let load = Task { await model.loadMore(.albums) }
        await client.waitForCallCount(2)
        await client.rejectCall(1, with: .pageFailed)
        await load.value

        XCTAssertEqual(model.phase, .results)
        XCTAssertEqual(model.artists.map(\.id), ["artist-kept"])
        XCTAssertEqual(model.albums.map(\.id), ["album-kept"])
        XCTAssertEqual(model.tracks.map(\.id), ["track-kept"])
        XCTAssertEqual(model.nextPageErrors, [.albums: "next page failed"])
    }

    func testSuspendedOldPaginationDoesNotBlockCurrentQueryPagination() async {
        let client = SuspendedSearchClient()
        let model = makeImmediateModel(client: client)
        model.setInput("old")
        await client.waitForCallCount(1)
        await client.resolveCall(0, with: page(albums: [album("old")], hasMoreAlbums: true))
        await waitUntil { model.phase == .results }

        let oldLoad = Task { await model.loadMore(.albums) }
        await client.waitForCallCount(2)

        model.setInput("new")
        await client.waitForCallCount(3)
        await client.resolveCall(2, with: page(albums: [album("new")], hasMoreAlbums: true))
        await waitUntil { model.albums.map(\.id) == ["album-new"] }

        let newLoad = Task { await model.loadMore(.albums) }
        for _ in 0 ..< 1_000 where await client.callCount() < 4 {
            await Task.yield()
        }
        let callCount = await client.callCount()
        XCTAssertEqual(callCount, 4)

        if callCount == 4 {
            await client.resolveCall(3, with: page(albums: [album("current-page")]))
        }
        await client.resolveCall(1, with: page(albums: [album("stale-page")]))
        await oldLoad.value
        await newLoad.value
    }

    func testCategoryLoadingIsObservableAndPreventsDuplicatePagination() async {
        let client = SuspendedSearchClient()
        let model = makeImmediateModel(client: client)
        model.setInput("loading")
        await client.waitForCallCount(1)
        await client.resolveCall(0, with: page(albums: [album("1")], hasMoreAlbums: true))
        await waitUntil { model.phase == .results }

        let load = Task { await model.loadMore(.albums) }
        await client.waitForCallCount(2)

        XCTAssertTrue(model.isLoading(.albums))
        XCTAssertFalse(model.isLoading(.artists))
        XCTAssertFalse(model.isLoading(.tracks))

        let duplicate = Task { await model.loadMore(.albums) }
        await Task.yield()
        let callCount = await client.callCount()
        XCTAssertEqual(callCount, 2)

        await client.resolveCall(1, with: page())
        await load.value
        await duplicate.value
        XCTAssertFalse(model.isLoading(.albums))
    }

    func testOldGenerationDeferCannotClearCurrentCategoryLoading() async {
        let client = SuspendedSearchClient()
        let model = makeImmediateModel(client: client)
        model.setInput("old")
        await client.waitForCallCount(1)
        await client.resolveCall(0, with: page(albums: [album("old")], hasMoreAlbums: true))
        await waitUntil { model.phase == .results }

        let oldLoad = Task { await model.loadMore(.albums) }
        await client.waitForCallCount(2)
        XCTAssertTrue(model.isLoading(.albums))

        model.setInput("new")
        XCTAssertFalse(model.isLoading(.albums))
        await client.waitForCallCount(3)
        await client.resolveCall(2, with: page(albums: [album("new")], hasMoreAlbums: true))
        await waitUntil { model.albums.map(\.id) == ["album-new"] }

        let currentLoad = Task { await model.loadMore(.albums) }
        await client.waitForCallCount(4)
        XCTAssertTrue(model.isLoading(.albums))

        await client.resolveCall(1, with: page())
        await oldLoad.value
        XCTAssertTrue(model.isLoading(.albums))

        await client.resolveCall(3, with: page())
        await currentLoad.value
        XCTAssertFalse(model.isLoading(.albums))
    }

    func testInitialFailureUsesFailedPhaseAndRetryCanRecover() async {
        let client = SuspendedSearchClient()
        let model = makeImmediateModel(client: client)
        model.setInput("retry")
        await client.waitForCallCount(1)
        await client.rejectCall(0, with: .initialFailed)
        await waitUntil { model.phase == .failed("initial failed") }

        model.retryInitial()
        await client.waitForCallCount(2)
        let retryRequest = await client.request(at: 1)
        XCTAssertEqual(retryRequest.query, "retry")
        await client.resolveCall(1, with: page(tracks: [track("recovered")]))
        await waitUntil { model.phase == .results }

        XCTAssertEqual(model.tracks.map(\.id), ["track-recovered"])
        XCTAssertTrue(model.nextPageErrors.isEmpty)
    }

    private func makeImmediateModel(client: SuspendedSearchClient) -> LibrarySearchViewModel {
        LibrarySearchViewModel(client: client) { duration in
            XCTAssertEqual(duration, .milliseconds(250))
        }
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

private actor ControlledSearchSleeper {
    private var durations: [Duration] = []
    private var continuations: [CheckedContinuation<Void, Error>] = []
    private var countWaiters: [(Int, CheckedContinuation<Void, Never>)] = []

    func callAsFunction(_ duration: Duration) async throws {
        durations.append(duration)
        resumeCountWaiters()
        try await withCheckedThrowingContinuation { continuations.append($0) }
    }

    func duration(at index: Int) -> Duration { durations[index] }

    func waitForCallCount(_ count: Int) async {
        guard durations.count < count else { return }
        await withCheckedContinuation { countWaiters.append((count, $0)) }
    }

    func resolveCall(_ index: Int) {
        continuations[index].resume()
    }

    private func resumeCountWaiters() {
        let ready = countWaiters.filter { $0.0 <= durations.count }
        countWaiters.removeAll { $0.0 <= durations.count }
        ready.forEach { $0.1.resume() }
    }
}

private actor SuspendedSearchClient: MusicClientProviding {
    private var requests: [SearchPageRequest] = []
    private var continuations: [CheckedContinuation<SearchPage, Error>] = []
    private var countWaiters: [(Int, CheckedContinuation<Void, Never>)] = []

    func searchPage(request: SearchPageRequest) async throws -> SearchPage {
        requests.append(request)
        resumeCountWaiters()
        return try await withCheckedThrowingContinuation { continuations.append($0) }
    }

    func callCount() -> Int { requests.count }
    func request(at index: Int) -> SearchPageRequest { requests[index] }

    func waitForCallCount(_ count: Int) async {
        guard requests.count < count else { return }
        await withCheckedContinuation { countWaiters.append((count, $0)) }
    }

    func resolveCall(_ index: Int, with response: SearchPage) {
        continuations[index].resume(returning: response)
    }

    func rejectCall(_ index: Int, with error: SearchTestError) {
        continuations[index].resume(throwing: error)
    }

    private func resumeCountWaiters() {
        let ready = countWaiters.filter { $0.0 <= requests.count }
        countWaiters.removeAll { $0.0 <= requests.count }
        ready.forEach { $0.1.resume() }
    }

    func login(server: String, user: String, password: String) async throws -> SessionValue {
        throw SearchTestError.unsupported
    }

    func logout() async {}

    func listAlbums(offset: UInt32, size: UInt32) async throws -> [Album] {
        throw SearchTestError.unsupported
    }

    func search(query: String) async throws -> SearchResult {
        throw SearchTestError.unsupported
    }

    func upload(localPath: String, libraryKey: String, progress: UploadProgress) async throws -> Track {
        throw SearchTestError.unsupported
    }

    func updateTags(id: String, update: TagUpdate) async throws { throw SearchTestError.unsupported }
    func deleteTrack(id: String) async throws { throw SearchTestError.unsupported }
    func moveTrack(id: String, key: String) async throws { throw SearchTestError.unsupported }
    func startScan() async throws -> ScanStatus { throw SearchTestError.unsupported }
    func scanStatus() async throws -> ScanStatus { throw SearchTestError.unsupported }
}

private enum SearchTestError: LocalizedError {
    case initialFailed
    case pageFailed
    case unsupported

    var errorDescription: String? {
        switch self {
        case .initialFailed: "initial failed"
        case .pageFailed: "next page failed"
        case .unsupported: "unsupported"
        }
    }
}

private func request(
    query: String,
    artistOffset: UInt32 = 0,
    artistCount: UInt32 = 0,
    albumOffset: UInt32 = 0,
    albumCount: UInt32 = 0,
    trackOffset: UInt32 = 0,
    trackCount: UInt32 = 0
) -> SearchPageRequest {
    SearchPageRequest(
        query: query,
        artistOffset: artistOffset,
        artistCount: artistCount,
        albumOffset: albumOffset,
        albumCount: albumCount,
        trackOffset: trackOffset,
        trackCount: trackCount
    )
}

private func page(
    artists: [Artist] = [],
    albums: [Album] = [],
    tracks: [Track] = [],
    hasMoreArtists: Bool = false,
    hasMoreAlbums: Bool = false,
    hasMoreTracks: Bool = false
) -> SearchPage {
    SearchPage(
        artists: artists,
        albums: albums,
        tracks: tracks,
        hasMoreArtists: hasMoreArtists,
        hasMoreAlbums: hasMoreAlbums,
        hasMoreTracks: hasMoreTracks
    )
}

private func artist(_ id: String) -> Artist {
    Artist(
        id: "artist-\(id)",
        name: "Artist \(id)",
        sortName: nil,
        coverArt: nil,
        musicBrainzId: nil,
        albumCount: 0
    )
}

private func album(_ id: String) -> Album {
    Album(
        id: "album-\(id)",
        name: "Album \(id)",
        artist: nil,
        artistId: nil,
        coverArt: nil,
        songCount: 0,
        duration: 0,
        year: nil,
        genre: nil,
        created: nil
    )
}

private func track(_ id: String) -> Track {
    Track(
        id: "track-\(id)",
        title: "Track \(id)",
        album: nil,
        albumId: nil,
        artist: nil,
        artistId: nil,
        track: nil,
        discNumber: nil,
        year: nil,
        genre: nil,
        coverArt: nil,
        size: 0,
        contentType: nil,
        suffix: nil,
        duration: 0,
        bitRate: 0,
        created: nil,
        path: nil
    )
}
