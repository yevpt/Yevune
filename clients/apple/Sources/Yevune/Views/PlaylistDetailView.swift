import SwiftUI
import YevuneCoreFFI

struct PlaylistDetailView: View {
    let detail: PlaylistDetail
    @ObservedObject var playlists: PlaylistViewModel
    @ObservedObject var media: MediaViewModel

    @State private var nameText = ""
    @State private var commentText = ""

    var body: some View {
        List {
            Section {
                TextField("歌单名称", text: $nameText)
                    .textFieldStyle(.roundedBorder)
                    .onSubmit {
                        let name = nameText.trimmingCharacters(in: .whitespacesAndNewlines)
                        if !name.isEmpty {
                            Task { await playlists.rename(playlistID: detail.playlist.id, name: name) }
                        }
                    }
                TextField("备注", text: $commentText)
                    .textFieldStyle(.roundedBorder)
                    .onSubmit {
                        Task { await playlists.setComment(playlistID: detail.playlist.id, comment: commentText) }
                    }
                if let error = playlists.errorMessage {
                    Text(error).foregroundStyle(.red)
                }
            }

            Section(detail.playlist.name) {
                if detail.tracks.isEmpty {
                    Text("歌单还没有曲目").foregroundStyle(.secondary)
                }
                ForEach(Array(detail.tracks.enumerated()), id: \.offset) { index, track in
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
        .task(id: detail.playlist.id) {
            nameText = detail.playlist.name
            commentText = detail.playlist.comment ?? ""
        }
    }
}
