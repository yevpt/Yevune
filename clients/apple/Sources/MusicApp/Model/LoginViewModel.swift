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
}

@MainActor
final class LoginViewModel: ObservableObject {
    @Published var server = ""
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
