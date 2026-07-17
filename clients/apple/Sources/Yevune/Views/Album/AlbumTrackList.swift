import SwiftUI
import YevuneCoreFFI

struct AlbumTrackList: View {
    let album: Album
    let tracks: [Track]
    let availableWidth: CGFloat
    let isAdmin: Bool
    @Binding var selection: Set<String>
    let onPlay: ([Track], Int) -> Void
    let onPlayNow: (Track) -> Void
    let onPlayNext: (Track) -> Void
    let onAddToQueue: (Track) -> Void
    let onAddToPlaylist: (Track) -> Void
    let onEditTags: ((Track) -> Void)?
    let onMove: ((Track) -> Void)?
    let onDelete: ((Track) -> Void)?
    let onManageAccess: ((Track) -> Void)?
    let onImportMusic: (() -> Void)?

    init(
        album: Album,
        tracks: [Track],
        availableWidth: CGFloat,
        isAdmin: Bool,
        selection: Binding<Set<String>>,
        onPlay: @escaping ([Track], Int) -> Void,
        onPlayNow: @escaping (Track) -> Void,
        onPlayNext: @escaping (Track) -> Void,
        onAddToQueue: @escaping (Track) -> Void,
        onAddToPlaylist: @escaping (Track) -> Void,
        onEditTags: ((Track) -> Void)? = nil,
        onMove: ((Track) -> Void)? = nil,
        onDelete: ((Track) -> Void)? = nil,
        onManageAccess: ((Track) -> Void)? = nil,
        onImportMusic: (() -> Void)? = nil
    ) {
        self.album = album
        self.tracks = tracks
        self.availableWidth = availableWidth
        self.isAdmin = isAdmin
        _selection = selection
        self.onPlay = onPlay
        self.onPlayNow = onPlayNow
        self.onPlayNext = onPlayNext
        self.onAddToQueue = onAddToQueue
        self.onAddToPlaylist = onAddToPlaylist
        self.onEditTags = onEditTags
        self.onMove = onMove
        self.onDelete = onDelete
        self.onManageAccess = onManageAccess
        self.onImportMusic = onImportMusic
    }

    private var columns: [AlbumWorkbenchColumn] {
        AlbumWorkbenchPolicy.columns(width: availableWidth)
    }

    private var gridMetrics: AlbumWorkbenchGridMetrics {
        AlbumWorkbenchPolicy.gridMetrics(width: availableWidth)
    }

    private var orderedTracks: [Track] {
        PlaybackViewPolicy.albumPlaybackOrder(tracks)
    }

    private var isMultiDisc: Bool {
        AlbumWorkbenchPolicy.isMultiDisc(tracks)
    }

    private var discGroups: [AlbumDiscGroup] {
        AlbumWorkbenchPolicy.discGroups(orderedTracks)
    }

    var body: some View {
        Group {
            if tracks.isEmpty {
                ContentUnavailableView {
                    Label("没有曲目", systemImage: "music.note.list")
                } description: {
                    Text(AlbumWorkbenchPolicy.emptyMessage(isAdmin: isAdmin))
                } actions: {
                    if isAdmin, let onImportMusic {
                        Button("导入音乐", action: onImportMusic)
                    }
                }
            } else {
                List(selection: $selection) {
                    ForEach(Array(discGroups.enumerated()), id: \.offset) { _, group in
                        if isMultiDisc {
                            Section("碟 \(group.discNumber)") {
                                trackRows(group.tracks)
                            }
                        } else {
                            Section {
                                trackRows(group.tracks)
                            }
                        }
                    }
                }
                .listStyle(.plain)
                .contentMargins(.horizontal, 0, for: .scrollContent)
                .focusable()
                .onKeyPress(.return) {
                    guard let selected = orderedTracks.first(where: { selection.contains($0.id) }) else {
                        return .ignored
                    }
                    play(selected)
                    return .handled
                }
                .onKeyPress(phases: .down) { keyPress in
                    guard keyPress.key == "a", keyPress.modifiers.contains(.command) else {
                        return .ignored
                    }
                    selection = Set(tracks.map(\.id))
                    return .handled
                }
            }
        }
        .onAppear(perform: reconcileSelection)
        .onChange(of: tracks.map(\.id)) { _, _ in reconcileSelection() }
    }

    @ViewBuilder
    private func trackRows(_ rows: [Track]) -> some View {
        ForEach(rows, id: \.id) { track in
            trackRow(track)
                .tag(track.id)
                .listRowInsets(
                    EdgeInsets(
                        top: 0,
                        leading: gridMetrics.outerHorizontalInset,
                        bottom: 0,
                        trailing: gridMetrics.outerHorizontalInset
                    )
                )
                .contentShape(Rectangle())
                .onTapGesture(count: 2) { play(track) }
                .contextMenu {
                    PlaybackTrackActions(
                        track: track,
                        onPlayNow: { onPlayNow(track) },
                        onPlayNext: { onPlayNext(track) },
                        onAddToQueue: { onAddToQueue(track) }
                    )
                    Divider()
                    Button("加入歌单…") { onAddToPlaylist(track) }

                    if isAdmin,
                       let onEditTags,
                       let onMove,
                       let onDelete,
                       let onManageAccess {
                        Divider()
                        Button("编辑标签…") { onEditTags(track) }
                        Button("移动…") { onMove(track) }
                        Button("设置曲目可见范围…") { onManageAccess(track) }
                        Divider()
                        Button("删除", role: .destructive) { onDelete(track) }
                    }
                }
        }
    }

    private func trackRow(_ track: Track) -> some View {
        Grid(horizontalSpacing: gridMetrics.horizontalSpacing, verticalSpacing: 0) {
            GridRow {
                Button { play(track) } label: {
                    Image(systemName: "play.fill")
                        .frame(width: gridMetrics.playButtonWidth)
                }
                .buttonStyle(.plain)
                .accessibilityLabel("播放 \(track.title)")

                ForEach(columns, id: \.self) { column in
                    trackCell(column, track: track)
                }
            }
        }
        .padding(.vertical, 3)
    }

    @ViewBuilder
    private func trackCell(_ column: AlbumWorkbenchColumn, track: Track) -> some View {
        switch column {
        case .trackNumber:
            Text(AlbumWorkbenchPolicy.trackNumber(track, isMultiDisc: isMultiDisc))
                .font(.callout.monospacedDigit())
                .foregroundStyle(.secondary)
                .frame(width: gridMetrics.trackNumberWidth, alignment: .trailing)
        case .titleAndArtist:
            VStack(alignment: .leading, spacing: 2) {
                Text(track.title).lineLimit(1)
                Text(track.artist ?? album.artist ?? "未知艺人")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        case .title:
            Text(track.title)
                .lineLimit(1)
                .frame(maxWidth: .infinity, alignment: .leading)
        case .artist:
            Text(track.artist ?? album.artist ?? "未知艺人")
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .frame(width: 150, alignment: .leading)
        case .duration:
            Text(formattedDuration(track.duration))
                .font(.callout.monospacedDigit())
                .foregroundStyle(.secondary)
                .frame(width: 58, alignment: .trailing)
        case .format:
            Text(track.suffix?.uppercased() ?? "—")
                .font(.caption)
                .foregroundStyle(.secondary)
                .frame(width: 56, alignment: .leading)
        }
    }

    private func play(_ track: Track) {
        let ordered = orderedTracks
        let index = ordered.firstIndex(where: { $0.id == track.id }) ?? 0
        onPlay(ordered, index)
    }

    private func reconcileSelection() {
        selection = AlbumWorkbenchPolicy.reconciledSelection(
            selection,
            trackIDs: tracks.map(\.id)
        )
    }

    private func formattedDuration(_ seconds: UInt32) -> String {
        guard seconds > 0 else { return "—" }
        let hours = seconds / 3_600
        let minutes = seconds % 3_600 / 60
        let remainingSeconds = seconds % 60
        if hours > 0 {
            return String(format: "%u:%02u:%02u", hours, minutes, remainingSeconds)
        }
        return String(format: "%u:%02u", minutes, remainingSeconds)
    }
}
