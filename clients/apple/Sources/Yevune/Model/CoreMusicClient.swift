import YevuneCoreFFI

actor CoreMusicClient: MusicClientProviding {
    private let client = YevuneCoreFFI.MusicClient()

    func login(server: String, user: String, password: String) async throws -> SessionValue {
        let session = try await client.login(server: server, user: user, password: password)
        return SessionValue(server: session.server, user: session.user, admin: session.admin)
    }

    func logout() async {
        await client.logout()
    }

    func listAlbums(offset: UInt32, size: UInt32) async throws -> [Album] {
        try await client.listAlbums(filter: .sort(.newest), offset: offset, size: size)
    }

    func listAlbums(filter: AlbumFilter, offset: UInt32, size: UInt32) async throws -> [Album] {
        try await client.listAlbums(filter: filter, offset: offset, size: size)
    }

    func listGenres() async throws -> [Genre] {
        try await client.listGenres()
    }

    func search(query: String) async throws -> SearchResult {
        try await client.search(query: query)
    }

    func searchPage(request: SearchPageRequest) async throws -> SearchPage {
        try await client.searchPage(request: request)
    }

    func upload(localPath: String, libraryKey: String, progress: UploadProgress) async throws -> Track {
        try await client.uploadTrack(
            localPath: localPath,
            metadata: UploadMetadata(libraryKey: libraryKey),
            progress: progress
        )
    }

    func updateTags(id: String, update: TagUpdate) async throws {
        try await client.updateTags(id: id, update: update)
    }

    func deleteTrack(id: String) async throws { try await client.deleteTrack(id: id) }
    func moveTrack(id: String, key: String) async throws { try await client.moveTrack(id: id, key: key) }
    func startScan() async throws -> ScanStatus { try await client.startScan() }
    func scanStatus() async throws -> ScanStatus { try await client.scanStatus() }
    func getAlbum(id: String) async throws -> AlbumDetail { try await client.getAlbum(id: id) }
    func coverArtURL(id: String, size: UInt32?) async throws -> String { try await client.coverArtUrl(id: id, size: size) }
    func setCoverArt(albumID: String, localPath: String) async throws { try await client.setCoverArt(albumId: albumID, localPath: localPath) }
    func streamURL(trackID: String) async throws -> String { try await client.streamUrl(trackId: trackID) }
    func getLyricsBySongID(_ id: String) async throws -> [StructuredLyrics] {
        try await client.getLyricsBySongId(id: id)
    }
    func scanPrefix(_ prefix: String) async throws -> DetailedScanResult { try await client.scanPrefix(prefix: prefix) }

    func playlistTree() async throws -> PlaylistTree { try await client.playlistTree() }
    func playlistDetail(id: String) async throws -> PlaylistDetail { try await client.playlistDetail(id: id) }
    func createPlaylist(name: String, folderID: String?, songIDs: [String]) async throws -> Playlist {
        try await client.createPlaylist(name: name, folderId: folderID, songIds: songIDs)
    }
    func renamePlaylist(id: String, name: String) async throws { try await client.renamePlaylist(id: id, name: name) }
    func setPlaylistComment(id: String, comment: String) async throws { try await client.setPlaylistComment(id: id, comment: comment) }
    func addTracks(id: String, songIDs: [String]) async throws { try await client.addTracks(id: id, songIds: songIDs) }
    func removeTrackAt(id: String, index: Int64) async throws { try await client.removeTrackAt(id: id, index: index) }
    func deletePlaylist(id: String) async throws { try await client.deletePlaylist(id: id) }
    func movePlaylist(id: String, folderID: String?) async throws { try await client.movePlaylist(id: id, folderId: folderID) }
    func createFolder(name: String, parentID: String?) async throws -> PlaylistFolder {
        try await client.createFolder(name: name, parentId: parentID)
    }
    func renameFolder(id: String, name: String) async throws { try await client.renameFolder(id: id, name: name) }
    func deleteFolder(id: String) async throws { try await client.deleteFolder(id: id) }
    func moveFolder(id: String, parentID: String?) async throws { try await client.moveFolder(id: id, parentId: parentID) }

    func listUsers() async throws -> [User] { try await client.listUsers() }
    func createUser(username: String, email: String, password: String, admin: Bool) async throws {
        try await client.createUser(username: username, email: email, password: password, admin: admin)
    }
    func updateUser(username: String, email: String, admin: Bool) async throws {
        try await client.updateUser(username: username, email: email, admin: admin)
    }
    func changePassword(username: String, password: String) async throws {
        try await client.changePassword(username: username, password: password)
    }
    func deleteUser(username: String) async throws { try await client.deleteUser(username: username) }
    func listRoles() async throws -> [Role] { try await client.listRoles() }
    func createRole(name: String) async throws -> Role { try await client.createRole(name: name) }
    func deleteRole(id: String) async throws { try await client.deleteRole(id: id) }
    func assignRole(userID: String, roleID: String) async throws {
        try await client.assignRole(userId: userID, roleId: roleID)
    }
    func unassignRole(userID: String, roleID: String) async throws {
        try await client.unassignRole(userId: userID, roleId: roleID)
    }

    func listAccessRules() async throws -> [AccessRule] { try await client.listAccessRules() }
    func setAccessRule(scopeType: ScopeType, scopeID: String, grants: [Principal]) async throws -> AccessRule {
        try await client.setAccessRule(scopeType: scopeType, scopeId: scopeID, grants: grants)
    }
    func deleteAccessRule(id: String) async throws { try await client.deleteAccessRule(id: id) }
    func getSong(id: String) async throws -> Track { try await client.getSong(id: id) }
    func getArtist(id: String) async throws -> ArtistDetail { try await client.getArtist(id: id) }
    func listArtists() async throws -> [Artist] { try await client.listArtists() }
}
