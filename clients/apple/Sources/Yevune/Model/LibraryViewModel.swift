import YevuneCoreFFI
import Foundation

enum LibraryViewMode: String, CaseIterable, Identifiable {
    case grid = "网格"
    case list = "列表"
    var id: String { rawValue }
}

@MainActor
final class LibraryViewModel: ObservableObject {
    @Published private(set) var albums: [Album] = []
    @Published private(set) var searchResult: SearchResult?
    @Published private(set) var errorMessage: String?
    @Published private(set) var isLoading = false
    @Published private(set) var genres: [Genre] = []

    @Published var sort: AlbumSort = .newest
    @Published var genreFilter: String?
    @Published var yearFilterEnabled = false
    @Published var fromYear: UInt32 = 2000
    @Published var toYear: UInt32 = UInt32(Calendar.current.component(.year, from: Date()))
    @Published var viewMode: LibraryViewMode = .grid

    private let client: any MusicClientProviding
    var clientForViews: any MusicClientProviding { client }

    init(client: any MusicClientProviding) {
        self.client = client
    }

    func load() async {
        isLoading = true
        errorMessage = nil
        defer { isLoading = false }
        do {
            albums = try await client.listAlbums(filter: currentFilter, offset: 0, size: 100)
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func loadGenres() async {
        do {
            genres = try await client.listGenres()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func search(query: String) async {
        guard !query.isEmpty else {
            searchResult = nil
            return
        }
        isLoading = true
        errorMessage = nil
        defer { isLoading = false }
        do {
            searchResult = try await client.search(query: query)
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func album(id: String?) -> Album? {
        guard let id else { return nil }
        return albums.first { $0.id == id }
    }

    private var currentFilter: AlbumFilter {
        if let genre = genreFilter { return .genre(genre) }
        if yearFilterEnabled { return .yearRange(from: fromYear, to: toYear) }
        return .sort(sort)
    }
}
