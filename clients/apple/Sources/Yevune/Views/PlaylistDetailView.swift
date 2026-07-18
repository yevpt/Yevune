import SwiftUI
import YevuneCoreFFI

struct PlaylistDetailView: View {
    let detail: PlaylistDetail
    @ObservedObject var playlists: PlaylistViewModel
    @ObservedObject var playback: PlaybackController

    @State private var selectedPositions: Set<Int> = []
    @State private var metadataPresented = false
    @State private var pendingRemoval: IndexSet?

    var body: some View {
        VStack(spacing: 0) {
            header
            statusBanner
            Divider()
            trackContent
            batchAccessory
        }
        .navigationTitle(detail.playlist.name)
        .sheet(isPresented: $metadataPresented) {
            PlaylistMetadataEditor(detail: detail, playlists: playlists)
        }
        .confirmationDialog(
            removalTitle,
            isPresented: removalIsPresented,
            titleVisibility: .visible
        ) {
            Button("移出歌单", role: .destructive, action: confirmRemoval)
            Button("取消", role: .cancel) { pendingRemoval = nil }
        } message: {
            Text("只会从此歌单移除，不会删除曲库中的歌曲。")
        }
        .onChange(of: detail.playlist.id) { _, _ in selectedPositions.removeAll() }
        .onChange(of: detail.tracks.count) { _, count in
            selectedPositions = selectedPositions.filter { $0 < count }
        }
    }

    private var header: some View {
        HStack(alignment: .center, spacing: 22) {
            RoundedRectangle(cornerRadius: 18, style: .continuous)
                .fill(
                    LinearGradient(
                        colors: [.accentColor.opacity(0.7), .indigo.opacity(0.7)],
                        startPoint: .topLeading,
                        endPoint: .bottomTrailing
                    )
                )
                .overlay {
                    Image(systemName: "music.note.list")
                        .font(.system(size: 42, weight: .medium))
                        .foregroundStyle(.white)
                }
                .frame(width: 116, height: 116)
                .shadow(color: .black.opacity(0.12), radius: 14, y: 7)
                .accessibilityHidden(true)

            VStack(alignment: .leading, spacing: 8) {
                Text("歌单")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.secondary)
                    .textCase(.uppercase)
                Text(detail.playlist.name)
                    .font(.largeTitle.bold())
                    .lineLimit(2)
                    .minimumScaleFactor(0.75)
                if let comment = detail.playlist.comment, !comment.isEmpty {
                    Text(comment)
                        .font(.body)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                }
                Text(playlistStats)
                    .font(.subheadline)
                    .foregroundStyle(.tertiary)
                    .monospacedDigit()
            }
            .accessibilityElement(children: .combine)

            Spacer(minLength: 12)

            ViewThatFits(in: .horizontal) {
                HStack(spacing: 10) { headerButtons(labels: true) }
                VStack(spacing: 10) { headerButtons(labels: false) }
            }
        }
        .padding(.horizontal, 24)
        .padding(.vertical, 20)
        .background(.bar.opacity(0.42))
    }

    @ViewBuilder
    private func headerButtons(labels: Bool) -> some View {
        Button {
            play(detail.tracks)
        } label: {
            if labels { Label("播放全部", systemImage: "play.fill") }
            else { Image(systemName: "play.fill") }
        }
        .buttonStyle(.borderedProminent)
        .disabled(detail.tracks.isEmpty || playlists.isMutating)
        .accessibilityLabel("播放全部")

        Button {
            play(detail.tracks.shuffled())
        } label: {
            if labels { Label("随机播放", systemImage: "shuffle") }
            else { Image(systemName: "shuffle") }
        }
        .buttonStyle(.bordered)
        .disabled(detail.tracks.isEmpty || playlists.isMutating)
        .accessibilityLabel("随机播放")

        Button { metadataPresented = true } label: {
            if labels { Label("编辑信息", systemImage: "pencil") }
            else { Image(systemName: "pencil") }
        }
        .buttonStyle(.bordered)
        .disabled(playlists.isMutating)
        .accessibilityLabel("编辑歌单信息")
    }

    @ViewBuilder
    private var statusBanner: some View {
        if playlists.isMutating {
            HStack(spacing: 8) {
                ProgressView().controlSize(.small)
                Text("正在保存歌单…")
                Spacer()
            }
            .font(.caption)
            .foregroundStyle(.secondary)
            .padding(.horizontal, 16)
            .padding(.vertical, 7)
        } else if let error = playlists.errorMessage {
            HStack(spacing: 8) {
                Image(systemName: "exclamationmark.triangle")
                Text(error).lineLimit(2)
                Spacer()
                Button("重新加载") {
                    Task { await playlists.openPlaylist(id: detail.playlist.id) }
                }
            }
            .font(.caption)
            .foregroundStyle(.secondary)
            .padding(.horizontal, 16)
            .padding(.vertical, 7)
        }
    }

    @ViewBuilder
    private var trackContent: some View {
        if detail.tracks.isEmpty {
            ContentUnavailableView {
                Label("歌单还没有歌曲", systemImage: "music.note.list")
            } description: {
                Text("从专辑的歌曲菜单或批量操作中选择“加入歌单”。")
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else {
            List(selection: $selectedPositions) {
                ForEach(Array(detail.tracks.enumerated()), id: \.offset) { index, track in
                    trackRow(track, index: index)
                        .tag(index)
                        .contextMenu {
                            PlaybackTrackActions(track: track, playback: playback)
                            Divider()
                            MediaAnnotationMenuActions(
                                target: .track(track.id),
                                starred: track.starred,
                                rating: track.userRating
                            )
                            Divider()
                            Button("移出歌单…", role: .destructive) {
                                pendingRemoval = IndexSet(integer: index)
                            }
                            .disabled(playlists.isMutating)
                        }
                }
                .onMove(perform: moveTracks)
            }
            .listStyle(.inset)
            .disabled(playlists.isMutating)
        }
    }

    private func trackRow(_ track: Track, index: Int) -> some View {
        HStack(spacing: 12) {
            Text("\(index + 1)")
                .font(.caption.monospacedDigit())
                .foregroundStyle(.tertiary)
                .frame(width: 30, alignment: .trailing)

            VStack(alignment: .leading, spacing: 3) {
                Text(track.title).lineLimit(1)
                Text(track.artist ?? "未知艺人")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            .frame(maxWidth: .infinity, alignment: .leading)

            if let album = track.album, !album.isEmpty {
                Text(album)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .frame(maxWidth: 180, alignment: .leading)
            }

            MediaAnnotationIndicator(
                target: .track(track.id),
                starred: track.starred,
                rating: track.userRating
            )

            if track.duration > 0 {
                Text(playbackTime(track.duration))
                    .font(.caption.monospacedDigit())
                    .foregroundStyle(.tertiary)
                    .frame(width: 44, alignment: .trailing)
            }

            Image(systemName: "line.3.horizontal")
                .foregroundStyle(.quaternary)
                .help("拖动调整顺序")
        }
        .padding(.vertical, 6)
        .contentShape(Rectangle())
        .accessibilityLabel("第 \(index + 1) 首，\(track.title)，\(track.artist ?? "未知艺人")")
        .accessibilityAction(named: "播放") { play(detail.tracks, startingAt: index) }
    }

    @ViewBuilder
    private var batchAccessory: some View {
        if !selectedPositions.isEmpty {
            HStack(spacing: 12) {
                Text("已选择 \(selectedPositions.count) 首")
                    .font(.headline)
                Spacer()
                Button("播放所选") {
                    play(PlaylistWorkbenchPolicy.selectedTracks(
                        detail.tracks,
                        positions: selectedPositions
                    ))
                }
                Button("移出歌单…", role: .destructive) {
                    pendingRemoval = IndexSet(selectedPositions)
                }
                Button("清除选择") { selectedPositions.removeAll() }
            }
            .padding(.horizontal, 18)
            .frame(height: 52)
            .background(.bar)
            .overlay(alignment: .top) { Divider() }
            .disabled(playlists.isMutating)
        }
    }

    private var playlistStats: String {
        "\(detail.tracks.count) 首 · \(playbackTime(detail.playlist.duration))"
    }

    private var removalTitle: String {
        let count = pendingRemoval?.count ?? 0
        return count > 1 ? "将这 \(count) 首歌曲移出歌单？" : "将这首歌曲移出歌单？"
    }

    private var removalIsPresented: Binding<Bool> {
        Binding(get: { pendingRemoval != nil }, set: { if !$0 { pendingRemoval = nil } })
    }

    private func play(_ tracks: [Track], startingAt index: Int = 0) {
        guard !tracks.isEmpty else { return }
        Task { await playback.play(tracks: tracks, startingAt: min(index, tracks.count - 1)) }
    }

    private func moveTracks(from source: IndexSet, to destination: Int) {
        let reordered = PlaylistWorkbenchPolicy.moving(
            detail.tracks,
            fromOffsets: source,
            toOffset: destination
        )
        selectedPositions.removeAll()
        Task {
            _ = await playlists.replaceTracks(
                playlistID: detail.playlist.id,
                tracks: reordered
            )
        }
    }

    private func confirmRemoval() {
        guard let pendingRemoval else { return }
        self.pendingRemoval = nil
        let remaining = PlaylistWorkbenchPolicy.removing(detail.tracks, offsets: pendingRemoval)
        selectedPositions.removeAll()
        Task {
            _ = await playlists.replaceTracks(
                playlistID: detail.playlist.id,
                tracks: remaining
            )
        }
    }
}

private struct PlaylistMetadataEditor: View {
    let detail: PlaylistDetail
    @ObservedObject var playlists: PlaylistViewModel
    @Environment(\.dismiss) private var dismiss
    @State private var name: String
    @State private var comment: String

    init(detail: PlaylistDetail, playlists: PlaylistViewModel) {
        self.detail = detail
        self.playlists = playlists
        _name = State(initialValue: detail.playlist.name)
        _comment = State(initialValue: detail.playlist.comment ?? "")
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            Text("编辑歌单信息").font(.title2.bold())

            Form {
                TextField("名称", text: $name)
                TextField("备注", text: $comment, axis: .vertical)
                    .lineLimit(3 ... 6)
            }
            .formStyle(.grouped)

            if metadata == nil {
                Text("歌单名称不能为空")
                    .font(.caption)
                    .foregroundStyle(.red)
            }
            if let error = playlists.errorMessage {
                Text(error)
                    .font(.caption)
                    .foregroundStyle(.red)
            }

            HStack {
                Spacer()
                Button("取消") { dismiss() }
                    .keyboardShortcut(.cancelAction)
                Button("保存") { save() }
                    .buttonStyle(.borderedProminent)
                    .keyboardShortcut(.defaultAction)
                    .disabled(metadata == nil || playlists.isMutating)
            }
        }
        .padding(24)
        .frame(width: 440)
        .interactiveDismissDisabled(playlists.isMutating)
    }

    private var metadata: PlaylistMetadata? {
        PlaylistWorkbenchPolicy.metadata(name: name, comment: comment)
    }

    private func save() {
        guard let metadata else { return }
        Task {
            if await playlists.saveMetadata(
                playlistID: detail.playlist.id,
                name: metadata.name,
                comment: metadata.comment
            ) {
                dismiss()
            }
        }
    }
}
