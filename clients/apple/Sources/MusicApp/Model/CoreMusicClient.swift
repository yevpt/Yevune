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
}
