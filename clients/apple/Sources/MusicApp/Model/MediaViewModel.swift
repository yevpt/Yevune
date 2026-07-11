import AVFoundation
import CoreFFI
import Foundation

@MainActor
final class MediaViewModel: ObservableObject {
    @Published private(set) var detail: AlbumDetail?
    @Published private(set) var coverURL: URL?
    @Published private(set) var errorMessage: String?
    @Published private(set) var playingTrackID: String?
    private let client: any MusicClientProviding
    private let player = AVPlayer()

    init(client: any MusicClientProviding) { self.client = client }

    func load(album: Album) async {
        do {
            detail = try await client.getAlbum(id: album.id)
            if let cover = album.coverArt {
                coverURL = URL(string: try await client.coverArtURL(id: cover, size: 600))
            } else { coverURL = nil }
        } catch { errorMessage = error.localizedDescription }
    }

    func replaceCover(albumID: String, path: String) async {
        do { try await client.setCoverArt(albumID: albumID, localPath: path) }
        catch { errorMessage = error.localizedDescription }
    }

    func toggle(track: Track) async {
        if playingTrackID == track.id { player.pause(); playingTrackID = nil; return }
        do {
            let url = try await client.streamURL(trackID: track.id)
            guard let url = URL(string: url) else { return }
            player.replaceCurrentItem(with: AVPlayerItem(url: url)); player.play(); playingTrackID = track.id
        } catch { errorMessage = error.localizedDescription }
    }
}
