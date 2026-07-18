import SwiftUI
import YevuneCoreFFI

struct PlaylistPickerSheet: View {
    @ObservedObject var playlists: PlaylistViewModel
    let trackIDs: [String]
    let onCancel: () -> Void
    let onAdded: () -> Void

    @State private var didAttempt = false

    var body: some View {
        NavigationStack {
            Group {
                if playlists.tree == nil {
                    ProgressView("正在加载歌单…")
                } else if let targets = playlists.tree?.playlists, !targets.isEmpty {
                    List(targets, id: \.id) { playlist in
                        Button {
                            add(to: playlist)
                        } label: {
                            HStack(spacing: 12) {
                                Image(systemName: "music.note.list")
                                    .foregroundStyle(.secondary)
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(playlist.name)
                                    Text("\(playlist.songCount) 首")
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                }
                                Spacer()
                            }
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .disabled(playlists.isMutating)
                    }
                } else {
                    ContentUnavailableView {
                        Label("还没有歌单", systemImage: "music.note.list")
                    } description: {
                        Text("请先在侧栏新建歌单，再把歌曲加入其中。")
                    }
                }
            }
            .overlay {
                if playlists.isMutating {
                    ProgressView("正在加入歌单…")
                        .padding(18)
                        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12))
                }
            }
            .safeAreaInset(edge: .bottom) {
                if didAttempt, let error = playlists.errorMessage {
                    Label(error, systemImage: "exclamationmark.triangle")
                        .font(.caption)
                        .foregroundStyle(.red)
                        .lineLimit(3)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(12)
                        .background(.bar)
                }
            }
            .navigationTitle(trackIDs.count == 1 ? "加入歌单" : "将 \(trackIDs.count) 首歌曲加入歌单")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("取消", action: onCancel)
                        .disabled(playlists.isMutating)
                }
            }
        }
        .frame(minWidth: 380, minHeight: 340)
        .interactiveDismissDisabled(playlists.isMutating)
    }

    private func add(to playlist: Playlist) {
        didAttempt = true
        Task {
            if await playlists.addTracks(playlistID: playlist.id, songIDs: trackIDs) {
                onAdded()
            }
        }
    }
}
