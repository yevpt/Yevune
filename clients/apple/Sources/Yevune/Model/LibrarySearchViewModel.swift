import Foundation
import YevuneCoreFFI

typealias SearchSleeper = @Sendable (Duration) async throws -> Void

enum LibrarySearchPhase: Equatable {
    case idle
    case debouncing
    case loading
    case results
    case empty
    case failed(String)
}

enum SearchResultCategory: CaseIterable {
    case artists
    case albums
    case tracks
}

@MainActor
final class LibrarySearchViewModel: ObservableObject {
    static let pageSize: UInt32 = 24

    @Published private(set) var input = ""
    @Published private(set) var query = ""
    @Published private(set) var phase: LibrarySearchPhase = .idle
    @Published private(set) var artists: [Artist] = []
    @Published private(set) var albums: [Album] = []
    @Published private(set) var tracks: [Track] = []
    @Published private(set) var hasMoreArtists = false
    @Published private(set) var hasMoreAlbums = false
    @Published private(set) var hasMoreTracks = false
    @Published private(set) var nextPageErrors: [SearchResultCategory: String] = [:]

    private let client: any MusicClientProviding
    private let sleeper: SearchSleeper
    private var task: Task<Void, Never>?
    private var generation = 0
    private var loadingCategoryGenerations: [SearchResultCategory: Int] = [:]

    init(
        client: any MusicClientProviding,
        sleeper: @escaping SearchSleeper = { duration in
            try await Task.sleep(for: duration)
        }
    ) {
        self.client = client
        self.sleeper = sleeper
    }

    func setInput(_ value: String) {
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            clear()
            return
        }

        generation += 1
        task?.cancel()
        let capturedGeneration = generation
        input = value
        query = trimmed
        phase = .debouncing
        resetResults()

        task = Task { [client, sleeper] in
            do {
                try await sleeper(.milliseconds(250))
                try Task.checkCancellation()
                guard self.matches(generation: capturedGeneration, query: trimmed) else { return }
                self.phase = .loading

                let response = try await client.searchPage(request: Self.initialRequest(query: trimmed))
                guard self.matches(generation: capturedGeneration, query: trimmed) else { return }
                self.applyInitial(response)
            } catch is CancellationError {
                return
            } catch {
                guard self.matches(generation: capturedGeneration, query: trimmed) else { return }
                self.phase = .failed(error.localizedDescription)
            }
        }
    }

    func retryInitial() {
        guard !query.isEmpty else { return }
        setInput(input)
    }

    func loadMore(_ category: SearchResultCategory) async {
        let capturedGeneration = generation
        let capturedQuery = query
        guard canLoadMore(category), loadingCategoryGenerations[category] != capturedGeneration else { return }
        loadingCategoryGenerations[category] = capturedGeneration
        defer {
            if loadingCategoryGenerations[category] == capturedGeneration {
                loadingCategoryGenerations[category] = nil
            }
        }

        let request = nextPageRequest(for: category, query: capturedQuery)
        nextPageErrors[category] = nil

        do {
            let response = try await client.searchPage(request: request)
            guard matches(generation: capturedGeneration, query: capturedQuery) else { return }
            applyNextPage(response, category: category)
        } catch {
            guard matches(generation: capturedGeneration, query: capturedQuery) else { return }
            nextPageErrors[category] = error.localizedDescription
        }
    }

    func clear() {
        generation += 1
        task?.cancel()
        task = nil
        loadingCategoryGenerations = [:]
        input = ""
        query = ""
        phase = .idle
        resetResults()
    }

    private func resetResults() {
        artists = []
        albums = []
        tracks = []
        hasMoreArtists = false
        hasMoreAlbums = false
        hasMoreTracks = false
        nextPageErrors = [:]
    }

    private func applyInitial(_ response: SearchPage) {
        artists = appendingUnique([], response.artists, id: \.id)
        albums = appendingUnique([], response.albums, id: \.id)
        tracks = appendingUnique([], response.tracks, id: \.id)
        hasMoreArtists = response.hasMoreArtists
        hasMoreAlbums = response.hasMoreAlbums
        hasMoreTracks = response.hasMoreTracks
        nextPageErrors = [:]
        phase = artists.isEmpty && albums.isEmpty && tracks.isEmpty ? .empty : .results
    }

    private func applyNextPage(_ response: SearchPage, category: SearchResultCategory) {
        switch category {
        case .artists:
            artists = appendingUnique(artists, response.artists, id: \.id)
            hasMoreArtists = response.hasMoreArtists
        case .albums:
            albums = appendingUnique(albums, response.albums, id: \.id)
            hasMoreAlbums = response.hasMoreAlbums
        case .tracks:
            tracks = appendingUnique(tracks, response.tracks, id: \.id)
            hasMoreTracks = response.hasMoreTracks
        }
        nextPageErrors[category] = nil
    }

    private func canLoadMore(_ category: SearchResultCategory) -> Bool {
        guard !query.isEmpty else { return false }
        switch category {
        case .artists: return hasMoreArtists
        case .albums: return hasMoreAlbums
        case .tracks: return hasMoreTracks
        }
    }

    private func nextPageRequest(for category: SearchResultCategory, query: String) -> SearchPageRequest {
        switch category {
        case .artists:
            return SearchPageRequest(
                query: query,
                artistOffset: UInt32(artists.count),
                artistCount: Self.pageSize,
                albumOffset: 0,
                albumCount: 0,
                trackOffset: 0,
                trackCount: 0
            )
        case .albums:
            return SearchPageRequest(
                query: query,
                artistOffset: 0,
                artistCount: 0,
                albumOffset: UInt32(albums.count),
                albumCount: Self.pageSize,
                trackOffset: 0,
                trackCount: 0
            )
        case .tracks:
            return SearchPageRequest(
                query: query,
                artistOffset: 0,
                artistCount: 0,
                albumOffset: 0,
                albumCount: 0,
                trackOffset: UInt32(tracks.count),
                trackCount: Self.pageSize
            )
        }
    }

    private func matches(generation expectedGeneration: Int, query expectedQuery: String) -> Bool {
        generation == expectedGeneration && query == expectedQuery
    }

    private func appendingUnique<T>(_ current: [T], _ incoming: [T], id: (T) -> String) -> [T] {
        var seen = Set(current.map(id))
        return current + incoming.filter { seen.insert(id($0)).inserted }
    }

    private static func initialRequest(query: String) -> SearchPageRequest {
        SearchPageRequest(
            query: query,
            artistOffset: 0,
            artistCount: pageSize,
            albumOffset: 0,
            albumCount: pageSize,
            trackOffset: 0,
            trackCount: pageSize
        )
    }
}
