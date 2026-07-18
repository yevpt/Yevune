import Foundation
import YevuneCoreFFI

enum FavoriteLibrarySection: String, CaseIterable, Identifiable {
    case tracks = "歌曲"
    case albums = "专辑"
    case artists = "艺人"

    var id: Self { self }
}

@MainActor
final class FavoriteLibraryViewModel: ObservableObject {
    @Published private(set) var tracks: [Track] = []
    @Published private(set) var albums: [Album] = []
    @Published private(set) var artists: [Artist] = []
    @Published private(set) var isLoading = false
    @Published private(set) var initialError: String?
    @Published private(set) var refreshError: String?
    @Published var section: FavoriteLibrarySection = .tracks

    private let client: any FavoriteLibraryProviding
    private var requestTask: Task<Void, Never>?
    private var generation = 0

    init(client: any FavoriteLibraryProviding) {
        self.client = client
    }

    func load() async {
        if let requestTask {
            await requestTask.value
            return
        }
        await startLoad()
    }

    func refresh() async {
        generation += 1
        requestTask?.cancel()
        requestTask = nil
        await startLoad()
    }

    func remove(_ target: MediaAnnotationTarget) {
        switch target {
        case .track(let id): tracks.removeAll { $0.id == id }
        case .album(let id): albums.removeAll { $0.id == id }
        case .artist(let id): artists.removeAll { $0.id == id }
        }
    }

    var isEmpty: Bool {
        tracks.isEmpty && albums.isEmpty && artists.isEmpty
    }

    private func startLoad() async {
        let expectedGeneration = generation
        let hadContent = !isEmpty
        isLoading = true
        initialError = nil
        refreshError = nil
        let task = Task { [client] in
            do {
                let response = try await client.getStarred()
                guard expectedGeneration == self.generation else { return }
                self.tracks = response.tracks
                self.albums = response.albums
                self.artists = response.artists
            } catch is CancellationError {
                return
            } catch {
                guard expectedGeneration == self.generation else { return }
                let message = LibraryOperationErrorPresentation.message(error)
                if hadContent { self.refreshError = message }
                else { self.initialError = message }
            }
            guard expectedGeneration == self.generation else { return }
            self.isLoading = false
            self.requestTask = nil
        }
        requestTask = task
        await task.value
    }
}
