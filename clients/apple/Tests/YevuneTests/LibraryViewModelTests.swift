import XCTest
import YevuneCoreFFI
@testable import Yevune

@MainActor
final class LibraryViewModelTests: XCTestCase {
    func testLoadUsesSortFilterByDefault() async {
        let fake = FakeBrowseClient(albums: [albumFixture(id: "al-1", name: "Blue")])
        let model = LibraryViewModel(client: fake)

        await model.load()

        XCTAssertEqual(fake.lastFilter, .sort(.newest))
        XCTAssertEqual(model.albums.map(\.id), ["al-1"])
        XCTAssertNil(model.errorMessage)
    }

    func testSettingGenreFilterSwitchesToGenreQuery() async {
        let fake = FakeBrowseClient()
        let model = LibraryViewModel(client: fake)
        model.genreFilter = "Rock"

        await model.load()

        XCTAssertEqual(fake.lastFilter, .genre("Rock"))
    }

    func testEnablingYearRangeSwitchesToYearQuery() async {
        let fake = FakeBrowseClient()
        let model = LibraryViewModel(client: fake)
        model.yearFilterEnabled = true
        model.fromYear = 2000
        model.toYear = 2010

        await model.load()

        XCTAssertEqual(fake.lastFilter, .yearRange(from: 2000, to: 2010))
    }

    func testLoadGenresPublishesGenreList() async {
        let fake = FakeBrowseClient(genres: [Genre(value: "Rock", songCount: 5, albumCount: 2)])
        let model = LibraryViewModel(client: fake)

        await model.loadGenres()

        XCTAssertEqual(model.genres.first?.value, "Rock")
        XCTAssertNil(model.errorMessage)
    }

    func testLoadFailureKeepsPreviousAlbumsAndSetsError() async {
        let fake = FakeBrowseClient(albums: [albumFixture(id: "al-1", name: "Blue")])
        let model = LibraryViewModel(client: fake)
        await model.load()

        fake.shouldFail = true
        await model.load()

        XCTAssertEqual(model.albums.map(\.id), ["al-1"])
        XCTAssertNotNil(model.errorMessage)
    }
}

private func albumFixture(id: String, name: String) -> Album {
    Album(id: id, name: name, artist: nil, artistId: nil, coverArt: nil,
          songCount: 0, duration: 0, year: nil, genre: nil, created: nil)
}

/// 记录最近一次筛选调用的假客户端，其余方法走协议默认实现。
private final class FakeBrowseClient: MusicClientProviding, @unchecked Sendable {
    var albums: [Album]
    var genres: [Genre]
    var shouldFail = false
    private(set) var lastFilter: AlbumFilter?

    init(albums: [Album] = [], genres: [Genre] = []) {
        self.albums = albums
        self.genres = genres
    }

    func login(server: String, user: String, password: String) async throws -> SessionValue {
        SessionValue(server: server, user: user)
    }
    func logout() async {}
    func listAlbums(offset: UInt32, size: UInt32) async throws -> [Album] { albums }
    func search(query: String) async throws -> SearchResult {
        SearchResult(artists: [], albums: albums, tracks: [])
    }
    func upload(localPath: String, libraryKey: String, progress: UploadProgress) async throws -> Track {
        throw CocoaError(.featureUnsupported)
    }
    func updateTags(id: String, update: TagUpdate) async throws {}
    func deleteTrack(id: String) async throws {}
    func moveTrack(id: String, key: String) async throws {}
    func startScan() async throws -> ScanStatus { throw CocoaError(.featureUnsupported) }
    func scanStatus() async throws -> ScanStatus { throw CocoaError(.featureUnsupported) }

    func listAlbums(filter: AlbumFilter, offset: UInt32, size: UInt32) async throws -> [Album] {
        if shouldFail { throw CocoaError(.fileReadUnknown) }
        lastFilter = filter
        return albums
    }
    func listGenres() async throws -> [Genre] {
        if shouldFail { throw CocoaError(.fileReadUnknown) }
        return genres
    }
}
