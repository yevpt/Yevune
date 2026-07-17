import SwiftUI
import YevuneCoreFFI
import UniformTypeIdentifiers

struct MediaDetailView: View {
    let album: Album
    @ObservedObject var model: MediaViewModel
    @ObservedObject var playlists: PlaylistViewModel
    @ObservedObject var playback: PlaybackController
    let isAdmin: Bool
    let onImportMusic: () -> Void
    let onManageAccess: ((AccessScopeTarget) -> Void)?

    @StateObject private var batch: TrackBatchOperationController
    @State private var selectedTrackIDs: Set<String> = []
    @State private var playlistTrackIDs: [String]?
    @State private var importing = false
    @State private var tagEditor: TagEditorViewModel?
    @State private var moveEditor: MoveTrackViewModel?
    @State private var batchEditorTracks: [Track]?
    @State private var pendingDeletion: DeletionTarget?
    @State private var showingBatchResults = false

    init(
        album: Album,
        model: MediaViewModel,
        playlists: PlaylistViewModel,
        playback: PlaybackController,
        isAdmin: Bool,
        onImportMusic: @escaping () -> Void = {},
        onManageAccess: ((AccessScopeTarget) -> Void)? = nil
    ) {
        self.album = album
        self.model = model
        self.playlists = playlists
        self.playback = playback
        self.isAdmin = isAdmin
        self.onImportMusic = onImportMusic
        self.onManageAccess = onManageAccess
        _batch = StateObject(wrappedValue: model.makeBatchController())
    }

    var body: some View {
        Group {
            if isAdmin {
                adminSurface
            } else {
                memberSurface
            }
        }
        .sheet(isPresented: playlistSheetIsPresented) {
            PlaylistPickerSheet(playlists: playlists, trackIDs: playlistTrackIDs ?? []) {
                playlistTrackIDs = nil
            }
        }
        .task(id: album.id) { await model.load(album: album) }
    }

    private var memberSurface: some View {
        detailSurface(isAdmin: false)
            .onChange(of: album.id) { _, newID in
                selectedTrackIDs.removeAll()
                playlistTrackIDs = nil
                batch.reset(for: newID)
            }
            .onAppear { resetBatchIfNeeded() }
    }

    private var adminSurface: some View {
        detailSurface(isAdmin: true)
            .fileImporter(isPresented: $importing, allowedContentTypes: [.image]) { result in
                guard !batch.isRunning else { return }
                guard case let .success(url) = result else { return }
                Task { await model.replaceCover(album: album, path: url.path) }
            }
            .sheet(isPresented: tagSheetIsPresented) {
                if let tagEditor {
                    TagEditorView(model: tagEditor) { message in
                        self.tagEditor = nil
                        refreshCurrentAlbum(message)
                    }
                }
            }
            .sheet(isPresented: moveSheetIsPresented) {
                if let moveEditor {
                    MoveTrackView(model: moveEditor) { message in
                        self.moveEditor = nil
                        refreshCurrentAlbum(message)
                    }
                }
            }
            .sheet(isPresented: batchEditorIsPresented) {
                if let tracks = batchEditorTracks {
                    BatchTagEditorView(
                        trackCount: tracks.count,
                        scopeExplanation: tracks.count == model.detail?.tracks.count
                            ? "专辑、艺人、流派和年份将作为公共字段应用到整张专辑。"
                            : nil
                    ) { update in
                        batchEditorTracks = nil
                        runBatch(tracks: tracks, action: .update(update), message: "批量标签已更新")
                    }
                }
            }
            .sheet(isPresented: $showingBatchResults) {
                BatchOperationResultView(
                    results: batch.results,
                    currentTrackID: batch.currentTrackID,
                    isRunning: batch.isRunning,
                    onStop: batch.stop,
                    onRetryFailed: retryFailedBatch,
                    onDone: { showingBatchResults = false }
                )
                .frame(minWidth: 500, minHeight: 360)
                .interactiveDismissDisabled(
                    !AlbumWorkbenchPolicy.canDismissBatchResults(isRunning: batch.isRunning)
                )
            }
            .confirmationDialog(
                deletionTitle,
                isPresented: deletionIsPresented,
                titleVisibility: .visible
            ) {
                Button("删除", role: .destructive, action: confirmDeletion)
                Button("取消", role: .cancel) { pendingDeletion = nil }
            } message: {
                Text(deletionMessage)
            }
            .onChange(of: album.id) { _, newID in
                clearAlbumState()
                batch.reset(for: newID)
            }
            .onAppear { resetBatchIfNeeded() }
    }

    private func detailSurface(isAdmin: Bool) -> some View {
        GeometryReader { geometry in
            VStack(spacing: 0) {
                switch (model.phase, model.detail) {
                case let (.failed(message), nil):
                    ContentUnavailableView {
                        Label("无法加载专辑", systemImage: "wifi.exclamationmark")
                    } description: {
                        Text(message)
                    } actions: {
                        Button("重试") { Task { await model.load(album: album) } }
                    }
                case (_, nil):
                    AlbumDetailSkeleton(album: album, availableWidth: geometry.size.width)
                case let (_, detail?):
                    if isAdmin {
                        adminContent(detail: detail, availableWidth: geometry.size.width)
                    } else {
                        memberContent(detail: detail, availableWidth: geometry.size.width)
                    }
                }
            }
        }
    }

    private func memberContent(detail: AlbumDetail, availableWidth: CGFloat) -> some View {
        VStack(spacing: 0) {
            sharedHeader(detail: detail, availableWidth: availableWidth)
            statusBanners
            memberTrackList(detail: detail, availableWidth: availableWidth)
            if !selectedTrackIDs.isEmpty {
                memberBatchActionBar(detail: detail)
            }
        }
    }

    private func adminContent(detail: AlbumDetail, availableWidth: CGFloat) -> some View {
        VStack(spacing: 0) {
            AlbumHeaderView(
                album: album,
                detail: detail,
                coverURL: model.coverURL,
                coverRevision: model.coverRevision,
                availableWidth: availableWidth,
                isAdmin: true,
                managementEnabled: managementEnabled,
                onPlay: { playAlbum(detail.tracks, startingAt: 0) },
                onReplaceCover: {
                    guard !batch.isRunning else { return }
                    importing = true
                },
                onManageAlbumAccess: albumAccessAction,
                onManageArtistAccess: artistAccessAction,
                onEditAlbum: {
                    guard !batch.isRunning else { return }
                    batchEditorTracks = ordered(detail.tracks)
                }
            )
            .padding(.vertical, 12)

            statusBanners

            AlbumTrackList(
                album: album,
                tracks: detail.tracks,
                availableWidth: availableWidth,
                isAdmin: true,
                managementEnabled: managementEnabled,
                selectionEnabled: selectionEnabled,
                selection: $selectedTrackIDs,
                onPlay: playAlbum,
                onPlayNow: { track in Task { await playback.playNow(track) } },
                onPlayNext: playback.playNext,
                onAddToQueue: playback.addToQueue,
                onAddToPlaylist: { playlistTrackIDs = [$0.id] },
                onEditTags: {
                    guard !batch.isRunning else { return }
                    tagEditor = model.makeTagEditor(for: $0)
                },
                onMove: {
                    guard !batch.isRunning else { return }
                    moveEditor = model.makeMoveEditor(for: $0)
                },
                onDelete: {
                    guard !batch.isRunning else { return }
                    pendingDeletion = .single($0)
                },
                onManageAccess: trackAccessAction,
                onImportMusic: onImportMusic
            )

            if !selectedTrackIDs.isEmpty {
                adminBatchActionBar(detail: detail)
            }
            if showsBatchResultReopen {
                HStack {
                    Spacer()
                    Button("查看批量结果") { showingBatchResults = true }
                    Spacer()
                }
                .padding(8)
                .background(.bar)
            }
        }
    }

    private func sharedHeader(detail: AlbumDetail, availableWidth: CGFloat) -> some View {
        AlbumHeaderView(
            album: album,
            detail: detail,
            coverURL: model.coverURL,
            coverRevision: model.coverRevision,
            availableWidth: availableWidth,
            isAdmin: false,
            onPlay: { playAlbum(detail.tracks, startingAt: 0) }
        )
        .padding(.vertical, 12)
    }

    private func memberTrackList(detail: AlbumDetail, availableWidth: CGFloat) -> some View {
        AlbumTrackList(
            album: album,
            tracks: detail.tracks,
            availableWidth: availableWidth,
            isAdmin: false,
            selection: $selectedTrackIDs,
            onPlay: playAlbum,
            onPlayNow: { track in Task { await playback.playNow(track) } },
            onPlayNext: playback.playNext,
            onAddToQueue: playback.addToQueue,
            onAddToPlaylist: { playlistTrackIDs = [$0.id] }
        )
    }

    private var statusBanners: some View {
        VStack(spacing: 0) {
            if model.phase == .refreshing {
                ProgressView().controlSize(.small).padding(.vertical, 4)
            }
            if let message = model.refreshError ?? model.coverError ?? model.operationError ?? playlists.errorMessage {
                HStack {
                    Image(systemName: "exclamationmark.triangle")
                    Text(message).lineLimit(2)
                    Spacer()
                    Button("重试") { Task { await model.load(album: album) } }
                }
                .font(.caption)
                .foregroundStyle(.secondary)
                .padding(8)
            }
            if let message = model.operationMessage {
                Label(message, systemImage: "checkmark.circle")
                    .font(.caption)
                    .foregroundStyle(.green)
                    .padding(8)
            }
        }
    }

    private func memberBatchActionBar(detail: AlbumDetail) -> some View {
        BatchActionBar(
            selectionCount: selectedTrackIDs.count,
            isAdmin: false,
            isRunning: batch.isRunning,
            onPlay: {
                let tracks = selectedTracks(in: detail)
                guard !tracks.isEmpty else { return }
                Task { await playback.play(tracks: tracks, startingAt: 0) }
            },
            onAddToPlaylist: { playlistTrackIDs = selectedTrackIDs.sorted() },
            onClearSelection: { selectedTrackIDs.removeAll() }
        )
    }

    private func adminBatchActionBar(detail: AlbumDetail) -> some View {
        BatchActionBar(
            selectionCount: selectedTrackIDs.count,
            isAdmin: true,
            isRunning: batch.isRunning,
            onPlay: {
                let tracks = selectedTracks(in: detail)
                guard !tracks.isEmpty else { return }
                Task { await playback.play(tracks: tracks, startingAt: 0) }
            },
            onAddToPlaylist: { playlistTrackIDs = selectedTrackIDs.sorted() },
            onClearSelection: { selectedTrackIDs.removeAll() },
            onEditTags: { batchEditorTracks = selectedTracks(in: detail) },
            onDelete: { pendingDeletion = .batch(selectedTracks(in: detail)) }
        )
    }

    private func playAlbum(_ tracks: [Track], startingAt index: Int) {
        let tracks = ordered(tracks)
        guard !tracks.isEmpty else { return }
        Task { await playback.play(tracks: tracks, startingAt: min(index, tracks.count - 1)) }
    }

    private func ordered(_ tracks: [Track]) -> [Track] {
        PlaybackViewPolicy.albumPlaybackOrder(tracks)
    }

    private func selectedTracks(in detail: AlbumDetail) -> [Track] {
        ordered(detail.tracks).filter { selectedTrackIDs.contains($0.id) }
    }

    private func refreshCurrentAlbum(_ message: String) {
        Task { await model.refreshAfterBatch(album: album, message: message) }
    }

    private func runBatch(tracks: [Track], action: TrackBatchAction, message: String) {
        guard !tracks.isEmpty else { return }
        showingBatchResults = true
        Task {
            await batch.run(tracks: tracks, action: action) {
                await model.refreshAfterBatch(album: album, message: message)
            }
        }
    }

    private func retryFailedBatch() {
        Task {
            await batch.retryFailed {
                await model.refreshAfterBatch(album: album, message: "批量操作已完成")
            }
        }
    }

    private func confirmDeletion() {
        guard !batch.isRunning else { return }
        guard let pendingDeletion else { return }
        self.pendingDeletion = nil
        switch pendingDeletion {
        case let .single(track):
            Task {
                if await model.deleteTrack(id: track.id, album: album) {
                    selectedTrackIDs.remove(track.id)
                }
            }
        case let .batch(tracks):
            runBatch(tracks: tracks, action: .delete, message: "批量删除已完成")
        }
    }

    private var deletionTitle: String {
        switch pendingDeletion {
        case let .single(track): "删除“\(track.title)”？"
        case let .batch(tracks): "删除所选 \(tracks.count) 首曲目？"
        case nil: "删除曲目？"
        }
    }

    private var deletionMessage: String {
        switch pendingDeletion {
        case let .single(track): "“\(track.title)”将从曲库中删除，且无法恢复。"
        case .batch: "这些曲目将从曲库中删除，且无法恢复。"
        case nil: "此操作无法恢复。"
        }
    }

    private var showsBatchResultReopen: Bool {
        AlbumWorkbenchPolicy.showsBatchResultReopen(
            resultCount: batch.results.count,
            isSheetPresented: showingBatchResults
        )
    }

    private var managementEnabled: Bool {
        AlbumWorkbenchPolicy.managementEnabled(isBatchRunning: batch.isRunning)
    }

    private var selectionEnabled: Bool {
        AlbumWorkbenchPolicy.selectionEnabled(isBatchRunning: batch.isRunning)
    }

    private var albumAccessAction: (() -> Void)? {
        guard let onManageAccess else { return nil }
        return {
            guard !batch.isRunning else { return }
            onManageAccess(.fromAlbum(album))
        }
    }

    private var artistAccessAction: (() -> Void)? {
        guard let onManageAccess,
              let target = AccessScopeTarget.artist(from: album) else { return nil }
        return {
            guard !batch.isRunning else { return }
            onManageAccess(target)
        }
    }

    private var trackAccessAction: ((Track) -> Void)? {
        guard let onManageAccess else { return nil }
        return { track in
            guard !batch.isRunning else { return }
            onManageAccess(.fromTrack(track))
        }
    }

    private func clearAlbumState() {
        selectedTrackIDs.removeAll()
        playlistTrackIDs = nil
        importing = false
        tagEditor = nil
        moveEditor = nil
        batchEditorTracks = nil
        pendingDeletion = nil
        showingBatchResults = false
    }

    private func resetBatchIfNeeded() {
        if batch.albumID != album.id { batch.reset(for: album.id) }
    }

    private var playlistSheetIsPresented: Binding<Bool> {
        Binding(get: { playlistTrackIDs != nil }, set: { if !$0 { playlistTrackIDs = nil } })
    }
    private var tagSheetIsPresented: Binding<Bool> {
        Binding(get: { tagEditor != nil }, set: { if !$0 { tagEditor = nil } })
    }
    private var moveSheetIsPresented: Binding<Bool> {
        Binding(get: { moveEditor != nil }, set: { if !$0 { moveEditor = nil } })
    }
    private var batchEditorIsPresented: Binding<Bool> {
        Binding(get: { batchEditorTracks != nil }, set: { if !$0 { batchEditorTracks = nil } })
    }
    private var deletionIsPresented: Binding<Bool> {
        Binding(get: { pendingDeletion != nil }, set: { if !$0 { pendingDeletion = nil } })
    }
}

private enum DeletionTarget {
    case single(Track)
    case batch([Track])
}

private struct AlbumDetailSkeleton: View {
    let album: Album
    let availableWidth: CGFloat

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack(alignment: .top, spacing: 20) {
                RoundedRectangle(cornerRadius: 8).fill(.quaternary)
                    .frame(width: availableWidth >= 620 ? 200 : 144, height: availableWidth >= 620 ? 200 : 144)
                VStack(alignment: .leading, spacing: 12) {
                    Text(album.name).font(availableWidth >= 620 ? .largeTitle : .title)
                    Text(album.artist ?? "未知艺人").font(.title3).foregroundStyle(.secondary)
                    ProgressView("正在加载曲目…")
                }
            }
            .padding(12)
            Divider()
            ForEach(0..<6, id: \.self) { _ in
                RoundedRectangle(cornerRadius: 3).fill(.quaternary)
                    .frame(height: 28).padding(.horizontal, 12)
            }
            Spacer()
        }
        .accessibilityLabel("正在加载专辑 \(album.name)")
    }
}

private struct PlaylistPickerSheet: View {
    @ObservedObject var playlists: PlaylistViewModel
    let trackIDs: [String]
    let onDismiss: () -> Void

    var body: some View {
        NavigationStack {
            List(playlists.tree?.playlists ?? [], id: \.id) { playlist in
                Button(playlist.name) {
                    Task {
                        await playlists.addTracks(playlistID: playlist.id, songIDs: trackIDs)
                        onDismiss()
                    }
                }
            }
            .navigationTitle("加入歌单")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("取消", action: onDismiss)
                }
            }
        }
        .frame(minWidth: 360, minHeight: 320)
    }
}
