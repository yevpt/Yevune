import CoreFFI

actor CoreMusicClient: MusicClientProviding {
    private let client = CoreFFI.MusicClient()

    func login(server: String, user: String, password: String) async throws -> SessionValue {
        let session = try await client.login(server: server, user: user, password: password)
        return SessionValue(server: session.server, user: session.user)
    }

    func listAlbums(offset: UInt32, size: UInt32) async throws -> [Album] {
        try await client.listAlbums(sort: .newest, offset: offset, size: size)
    }

    func search(query: String) async throws -> SearchResult {
        try await client.search(query: query)
    }
}
