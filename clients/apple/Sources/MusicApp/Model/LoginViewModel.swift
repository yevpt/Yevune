import Foundation

struct SessionValue: Equatable {
    let server: String
    let user: String
}

protocol MusicClientProviding: Sendable {
    func login(server: String, user: String, password: String) async throws -> SessionValue
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
