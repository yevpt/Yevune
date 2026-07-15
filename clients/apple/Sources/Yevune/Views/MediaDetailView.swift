import YevuneCoreFFI
import SwiftUI

struct MediaDetailView: View {
    let album: Album
    @ObservedObject var model: MediaViewModel
    @ObservedObject var playlists: PlaylistViewModel
    @ObservedObject var playback: PlaybackController
    let onManageAccess: ((AccessScopeTarget) -> Void)?
    @State private var importing = false
    @State private var selectedTrackIDs: Set<String> = []
    @State private var tagEditor: TagEditorViewModel?
    @State private var batchTagTrackIDs: [String]?
    @State private var pendingDeletion: [String]?

    init(
        album: Album,
        model: MediaViewModel,
        playlists: PlaylistViewModel,
        playback: PlaybackController,
        onManageAccess: ((AccessScopeTarget) -> Void)? = nil
    ) {
        self.album = album
        self.model = model
        self.playlists = playlists
        self.playback = playback
        self.onManageAccess = onManageAccess
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(alignment: .top) {
                AsyncImage(url: model.coverURL) { image in image.resizable().scaledToFill() } placeholder: { Color.secondary.opacity(0.15) }
                    .frame(width: 180, height: 180).clipped().cornerRadius(8)
                VStack(alignment: .leading) {
                    HStack {
                        Text(album.name).font(.largeTitle)
                        if let onManageAccess {
                            Menu {
                                Button("专辑可见范围") {
                                    onManageAccess(.fromAlbum(album))
                                }
                                if let artistTarget = AccessScopeTarget.artist(from: album) {
                                    Button("艺人可见范围") {
                                        onManageAccess(artistTarget)
                                    }
                                }
                            } label: {
                                Label("可见范围", systemImage: "eye")
                            }
                        }
                    }
                    Text(album.artist ?? "未知艺人").foregroundStyle(.secondary)
                    Button("替换封面") { importing = true }
                }
            }
            if let detail = model.detail {
                List(detail.tracks, id: \.id, selection: $selectedTrackIDs) { track in
                    HStack(spacing: 10) {
                        Button {
                            playAlbum(detail.tracks, from: track)
                        } label: {
                            Image(systemName: playback.currentTrack?.id == track.id && playback.engineState == .playing
                                ? "speaker.wave.2.fill"
                                : "play.fill")
                                .frame(width: 18)
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel("播放 \(track.title)")

                        VStack(alignment: .leading, spacing: 2) {
                            Text(track.title)
                            Text(track.artist ?? album.artist ?? "未知艺人")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                        Spacer()
                        if track.duration > 0 {
                            Text(playbackTime(track.duration))
                                .font(.caption.monospacedDigit())
                                .foregroundStyle(.tertiary)
                        }
                    }
                    .contentShape(Rectangle())
                    .onTapGesture(count: 2) {
                        playAlbum(detail.tracks, from: track)
                    }
                    .contextMenu {
                        PlaybackTrackActions(track: track, playback: playback)
                        Divider()
                        Button("编辑标签…") { tagEditor = model.makeTagEditor(for: track) }
                        Button("移动…") { tagEditor = model.makeTagEditor(for: track) }
                        Menu("加入歌单") {
                            ForEach(playlists.tree?.playlists ?? [], id: \.id) { pl in
                                Button(pl.name) { Task { await playlists.addTracks(playlistID: pl.id, songIDs: [track.id]) } }
                            }
                        }
                        if let onManageAccess {
                            Divider()
                            Button("设置曲目可见范围") {
                                onManageAccess(
                                    .fromTrack(track)
                                )
                            }
                        }
                        Divider()
                        Button("删除", role: .destructive) { pendingDeletion = [track.id] }
                    }
                }
            }
            if !selectedTrackIDs.isEmpty {
                HStack {
                    Text("已选择 \(selectedTrackIDs.count) 首")
                    Button("批量改标签…") { batchTagTrackIDs = selectedTrackIDs.sorted() }
                    Menu("加入歌单") {
                        ForEach(playlists.tree?.playlists ?? [], id: \.id) { playlist in
                            Button(playlist.name) {
                                let trackIDs = selectedTrackIDs.sorted()
                                Task {
                                    await playlists.addTracks(playlistID: playlist.id, songIDs: trackIDs)
                                    if playlists.errorMessage == nil {
                                        await model.refresh(album: album, successMessage: "已加入歌单")
                                    }
                                }
                            }
                        }
                    }
                    Button("批量删除", role: .destructive) { pendingDeletion = selectedTrackIDs.sorted() }
                }
            }
            if let message = model.operationMessage { Text(message).foregroundStyle(.green) }
            if let error = model.errorMessage ?? playlists.errorMessage { Text(error).foregroundStyle(.red) }
        }.padding().task(id: album.id) { await model.load(album: album) }
        .fileImporter(isPresented: $importing, allowedContentTypes: [.image]) { result in
            if case let .success(url) = result { Task { await model.replaceCover(albumID: album.id, path: url.path) } }
        }
        .sheet(isPresented: Binding(get: { tagEditor != nil }, set: { if !$0 { tagEditor = nil } })) {
            if let tagEditor {
                TagEditorView(model: tagEditor) { message in
                    self.tagEditor = nil
                    Task { await model.refresh(album: album, successMessage: message) }
                }
            }
        }
        .sheet(isPresented: Binding(get: { batchTagTrackIDs != nil }, set: { if !$0 { batchTagTrackIDs = nil } })) {
            if let trackIDs = batchTagTrackIDs {
                BatchTagEditorView(album: album, trackIDs: trackIDs, model: model) {
                    batchTagTrackIDs = nil
                }
            }
        }
        .confirmationDialog("确定删除所选曲目吗？", isPresented: Binding(get: { pendingDeletion != nil }, set: { if !$0 { pendingDeletion = nil } }), titleVisibility: .visible) {
            Button("删除", role: .destructive) {
                let trackIDs = pendingDeletion ?? []
                Task {
                    _ = await model.deleteTracks(ids: trackIDs, album: album)
                    selectedTrackIDs.subtract(trackIDs)
                    pendingDeletion = nil
                }
            }
        } message: {
            Text("将删除 \(pendingDeletion?.count ?? 0) 首曲目，此操作无法撤销。")
        }
        .onChange(of: album.id) { _, _ in selectedTrackIDs.removeAll() }
    }

    private func playAlbum(_ tracks: [Track], from track: Track) {
        let ordered = PlaybackViewPolicy.albumPlaybackOrder(tracks)
        let index = ordered.firstIndex { $0.id == track.id } ?? 0
        Task { await playback.play(tracks: ordered, startingAt: index) }
    }
}
