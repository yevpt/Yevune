import Foundation
import CoreFFI

struct SessionValue: Equatable {
    let server: String
    let user: String
}

protocol MusicClientProviding: Sendable {
    func login(server: String, user: String, password: String) async throws -> SessionValue
    func listAlbums(offset: UInt32, size: UInt32) async throws -> [Album]
    func search(query: String) async throws -> SearchResult
    func upload(localPath: String, libraryKey: String, progress: UploadProgress) async throws
    func updateTags(id: String, update: TagUpdate) async throws
    func deleteTrack(id: String) async throws
    func moveTrack(id: String, key: String) async throws
    func startScan() async throws -> ScanStatus
    func scanStatus() async throws -> ScanStatus
    func getAlbum(id: String) async throws -> AlbumDetail
    func coverArtURL(id: String, size: UInt32?) async throws -> String
    func setCoverArt(albumID: String, localPath: String) async throws
    func streamURL(trackID: String) async throws -> String
}

extension MusicClientProviding {
    func getAlbum(id: String) async throws -> AlbumDetail { throw CocoaError(.featureUnsupported) }
    func coverArtURL(id: String, size: UInt32?) async throws -> String { throw CocoaError(.featureUnsupported) }
    func setCoverArt(albumID: String, localPath: String) async throws { throw CocoaError(.featureUnsupported) }
    func streamURL(trackID: String) async throws -> String { throw CocoaError(.featureUnsupported) }
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
