import CoreFFI
import Foundation

@MainActor
final class LibraryViewModel: ObservableObject {
    @Published private(set) var albums: [Album] = []
    @Published private(set) var searchResult: SearchResult?
    @Published private(set) var errorMessage: String?
    @Published private(set) var isLoading = false

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
            albums = try await client.listAlbums(offset: 0, size: 100)
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
}
