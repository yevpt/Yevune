import SwiftUI
import CoreFFI

struct PlaylistDetailView: View {
    let detail: PlaylistDetail
    @ObservedObject var playlists: PlaylistViewModel
    @ObservedObject var media: MediaViewModel

    var body: some View {
        List {
            Section(detail.playlist.name) {
                if detail.tracks.isEmpty {
                    Text("歌单还没有曲目").foregroundStyle(.secondary)
                }
                ForEach(Array(detail.tracks.enumerated()), id: \.element.id) { index, track in
                    HStack {
                        VStack(alignment: .leading) {
                            Text(track.title)
                            Text(track.artist ?? "未知艺人").font(.caption).foregroundStyle(.secondary)
                        }
                        Spacer()
                        Button(media.playingTrackID == track.id ? "暂停" : "试听") {
                            Task { await media.toggle(track: track) }
                        }
                        .buttonStyle(.borderless)
                    }
                    .contextMenu {
                        Button("移出歌单", role: .destructive) {
                            Task { await playlists.removeTrack(playlistID: detail.playlist.id, index: Int64(index)) }
                        }
                    }
                }
            }
        }
        .navigationTitle(detail.playlist.name)
    }
}
