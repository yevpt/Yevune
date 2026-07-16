import Foundation
import YevuneCoreFFI

enum LibrarySection: String, CaseIterable {
    case albums
    case artists
}

enum AlbumBrowseCriterion: Equatable {
    case sort(AlbumSort)
    case genre(String)
    case yearRange(from: UInt32, to: UInt32)

    var filter: AlbumFilter? {
        switch self {
        case .sort(let value):
            return .sort(value)
        case .genre(let value):
            return .genre(value)
        case .yearRange(let from, let to):
            return from <= to ? .yearRange(from: from, to: to) : nil
        }
    }
}

@MainActor
final class LibraryBrowseViewModel: ObservableObject {
    static let albumPageSize: UInt32 = 60

    @Published private(set) var albums: [Album] = []
    @Published private(set) var artists: [Artist] = []
    @Published private(set) var genres: [Genre] = []
    @Published private(set) var hasMoreAlbums = true
    @Published private(set) var isRefreshing = false
    @Published private(set) var isLoadingNextPage = false
    @Published private(set) var initialError: String?
    @Published private(set) var refreshError: String?
    @Published private(set) var nextPageError: String?
    @Published private(set) var validationMessage: String?
    @Published private(set) var section: LibrarySection = .albums
    @Published private(set) var albumCriterion: AlbumBrowseCriterion = .sort(.newest)

    private let client: any MusicClientProviding
    private var requestTask: Task<Void, Never>?
    private var generation = 0

    init(client: any MusicClientProviding) {
        self.client = client
    }

    func reload() async {
        guard let task = startReload() else { return }
        await task.value
    }

    func loadNextPage() async {
        guard section == .albums,
              hasMoreAlbums,
              !isLoadingNextPage,
              !isRefreshing,
              let filter = albumCriterion.filter
        else { return }

        let capturedGeneration = generation
        let capturedSection = section
        let capturedCriterion = albumCriterion
        let offset = UInt32(albums.count)
        isLoadingNextPage = true
        nextPageError = nil

        let task = Task { [client] in
            do {
                let response = try await client.listAlbums(
                    filter: filter,
                    offset: offset,
                    size: Self.albumPageSize
                )
                guard self.matches(
                    generation: capturedGeneration,
                    section: capturedSection,
                    criterion: capturedCriterion
                ) else { return }

                self.albums = Self.stablyDeduplicated(self.albums + response)
                self.hasMoreAlbums = response.count == Int(Self.albumPageSize)
                self.isLoadingNextPage = false
            } catch {
                guard self.matches(
                    generation: capturedGeneration,
                    section: capturedSection,
                    criterion: capturedCriterion
                ) else { return }
                self.nextPageError = error.localizedDescription
                self.isLoadingNextPage = false
            }
        }
        requestTask = task
        await task.value
    }

    func selectSection(_ value: LibrarySection) {
        guard section != value else { return }
        section = value
        _ = startReload()
    }

    func selectCriterion(_ value: AlbumBrowseCriterion) {
        guard albumCriterion != value else { return }
        albumCriterion = value
        _ = startReload()
    }

    @discardableResult
    private func startReload() -> Task<Void, Never>? {
        generation += 1
        requestTask?.cancel()

        let capturedGeneration = generation
        let capturedSection = section
        let capturedCriterion = albumCriterion
        isLoadingNextPage = false
        initialError = nil
        refreshError = nil
        nextPageError = nil

        if capturedSection == .albums, capturedCriterion.filter == nil {
            validationMessage = "起始年份不能晚于结束年份"
            isRefreshing = false
            return nil
        }

        validationMessage = nil
        isRefreshing = true
        let hadContent = capturedSection == .albums ? !albums.isEmpty : !artists.isEmpty

        let task = Task { [client] in
            do {
                switch capturedSection {
                case .albums:
                    guard let filter = capturedCriterion.filter else { return }
                    let response = try await client.listAlbums(
                        filter: filter,
                        offset: 0,
                        size: Self.albumPageSize
                    )
                    let refreshedGenres = try? await client.listGenres()
                    guard self.matches(
                        generation: capturedGeneration,
                        section: capturedSection,
                        criterion: capturedCriterion
                    ) else { return }

                    self.albums = Self.stablyDeduplicated(response)
                    if let refreshedGenres {
                        self.genres = refreshedGenres
                    }
                    self.hasMoreAlbums = response.count == Int(Self.albumPageSize)
                case .artists:
                    let response = try await client.listArtists()
                    guard self.matches(
                        generation: capturedGeneration,
                        section: capturedSection,
                        criterion: capturedCriterion
                    ) else { return }

                    self.artists = response.sorted {
                        ($0.sortName ?? $0.name).localizedStandardCompare($1.sortName ?? $1.name) == .orderedAscending
                    }
                }
                self.isRefreshing = false
            } catch {
                guard self.matches(
                    generation: capturedGeneration,
                    section: capturedSection,
                    criterion: capturedCriterion
                ) else { return }
                if hadContent {
                    self.refreshError = error.localizedDescription
                } else {
                    self.initialError = error.localizedDescription
                }
                self.isRefreshing = false
            }
        }
        requestTask = task
        return task
    }

    private func matches(
        generation expectedGeneration: Int,
        section expectedSection: LibrarySection,
        criterion expectedCriterion: AlbumBrowseCriterion
    ) -> Bool {
        generation == expectedGeneration
            && section == expectedSection
            && albumCriterion == expectedCriterion
    }

    private static func stablyDeduplicated(_ albums: [Album]) -> [Album] {
        var seen = Set<String>()
        return albums.filter { seen.insert($0.id).inserted }
    }
}
