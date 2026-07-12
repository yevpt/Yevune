import Foundation
import CoreFFI

struct SessionValue: Equatable {
    let server: String
    let user: String
}

protocol MusicClientProviding: Sendable {
    func login(server: String, user: String, password: String) async throws -> SessionValue
    func listAlbums(offset: UInt32, size: UInt32) async throws -> [Album]
    func listAlbums(filter: AlbumFilter, offset: UInt32, size: UInt32) async throws -> [Album]
    func listGenres() async throws -> [Genre]
    func search(query: String) async throws -> SearchResult
    func upload(localPath: String, libraryKey: String, progress: UploadProgress) async throws -> Track
    func updateTags(id: String, update: TagUpdate) async throws
    func deleteTrack(id: String) async throws
    func moveTrack(id: String, key: String) async throws
    func startScan() async throws -> ScanStatus
    func scanStatus() async throws -> ScanStatus
    func getAlbum(id: String) async throws -> AlbumDetail
    func coverArtURL(id: String, size: UInt32?) async throws -> String
    func setCoverArt(albumID: String, localPath: String) async throws
    func streamURL(trackID: String) async throws -> String
    func scanPrefix(_ prefix: String) async throws -> DetailedScanResult
    func playlistTree() async throws -> PlaylistTree
    func playlistDetail(id: String) async throws -> PlaylistDetail
    func createPlaylist(name: String, folderID: String?, songIDs: [String]) async throws -> Playlist
    func renamePlaylist(id: String, name: String) async throws
    func setPlaylistComment(id: String, comment: String) async throws
    func addTracks(id: String, songIDs: [String]) async throws
    func removeTrackAt(id: String, index: Int64) async throws
    func deletePlaylist(id: String) async throws
    func movePlaylist(id: String, folderID: String?) async throws
    func createFolder(name: String, parentID: String?) async throws -> PlaylistFolder
    func renameFolder(id: String, name: String) async throws
    func deleteFolder(id: String) async throws
    func moveFolder(id: String, parentID: String?) async throws
}

extension MusicClientProviding {
    func listAlbums(filter: AlbumFilter, offset: UInt32, size: UInt32) async throws -> [Album] { throw CocoaError(.featureUnsupported) }
    func listGenres() async throws -> [Genre] { throw CocoaError(.featureUnsupported) }
    func getAlbum(id: String) async throws -> AlbumDetail { throw CocoaError(.featureUnsupported) }
    func coverArtURL(id: String, size: UInt32?) async throws -> String { throw CocoaError(.featureUnsupported) }
    func setCoverArt(albumID: String, localPath: String) async throws { throw CocoaError(.featureUnsupported) }
    func streamURL(trackID: String) async throws -> String { throw CocoaError(.featureUnsupported) }
    func scanPrefix(_ prefix: String) async throws -> DetailedScanResult { throw CocoaError(.featureUnsupported) }
    func playlistTree() async throws -> PlaylistTree { throw CocoaError(.featureUnsupported) }
    func playlistDetail(id: String) async throws -> PlaylistDetail { throw CocoaError(.featureUnsupported) }
    func createPlaylist(name: String, folderID: String?, songIDs: [String]) async throws -> Playlist { throw CocoaError(.featureUnsupported) }
    func renamePlaylist(id: String, name: String) async throws { throw CocoaError(.featureUnsupported) }
    func setPlaylistComment(id: String, comment: String) async throws { throw CocoaError(.featureUnsupported) }
    func addTracks(id: String, songIDs: [String]) async throws { throw CocoaError(.featureUnsupported) }
    func removeTrackAt(id: String, index: Int64) async throws { throw CocoaError(.featureUnsupported) }
    func deletePlaylist(id: String) async throws { throw CocoaError(.featureUnsupported) }
    func movePlaylist(id: String, folderID: String?) async throws { throw CocoaError(.featureUnsupported) }
    func createFolder(name: String, parentID: String?) async throws -> PlaylistFolder { throw CocoaError(.featureUnsupported) }
    func renameFolder(id: String, name: String) async throws { throw CocoaError(.featureUnsupported) }
    func deleteFolder(id: String) async throws { throw CocoaError(.featureUnsupported) }
    func moveFolder(id: String, parentID: String?) async throws { throw CocoaError(.featureUnsupported) }
}

@MainActor
final class LoginViewModel: ObservableObject {
    @Published var server = "http://localhost:4533"
    @Published var user = ""
    @Published var password = ""
    @Published private(set) var session: SessionValue?
    @Published private(set) var errorMessage: String?
    @Published private(set) var isSubmitting = false

    private let client: any MusicClientProviding

    init(client: any MusicClientProviding) {
        self.client = client
    }

    func submit() async {
        isSubmitting = true
        errorMessage = nil
        defer { isSubmitting = false }
        do {
            session = try await client.login(server: server, user: user, password: password)
        } catch {
            errorMessage = error.localizedDescription
        }
    }
}
