import SwiftUI
import YevuneCoreFFI

struct LibrarySearchSelectionPresentation: Equatable {
    let highlightedAlbumID: String?
    let highlightedArtistID: String?

    func isAlbumHighlighted(_ id: String) -> Bool { highlightedAlbumID == id }
    func isArtistHighlighted(_ id: String) -> Bool { highlightedArtistID == id }
}

struct LibrarySearchResultsView: View {
    @ObservedObject var model: LibrarySearchViewModel
    @ObservedObject var playback: PlaybackController
    @ObservedObject var playlists: PlaylistViewModel
    let client: any MusicClientProviding
    let highlightedAlbumID: String?
    let highlightedArtistID: String?
    let onHighlightArtist: (Artist) -> Void
    let onOpenArtist: (Artist) -> Void
    let onHighlightAlbum: (Album) -> Void
    let onOpenAlbum: (Album) -> Void

    @State private var selectedTrackIDs: Set<String> = []
    @State private var playlistTrackIDs: [String]?
    @State private var actionMessage: String?

    private var selection: LibrarySearchSelectionPresentation {
        LibrarySearchSelectionPresentation(
            highlightedAlbumID: highlightedAlbumID,
            highlightedArtistID: highlightedArtistID
        )
    }

    var body: some View {
        Group {
            switch model.phase {
            case .idle:
                EmptyView()
            case .debouncing, .loading:
                ProgressView("正在搜索…")
            case .empty:
                emptyState
            case .failed(let message):
                ContentUnavailableView {
                    Label("搜索失败", systemImage: "wifi.exclamationmark")
                } description: {
                    Text(message)
                } actions: {
                    Button("重试", action: model.retryInitial)
                }
            case .results:
                results
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .sheet(isPresented: playlistPickerIsPresented) {
            PlaylistPickerSheet(
                playlists: playlists,
                trackIDs: playlistTrackIDs ?? [],
                onCancel: { playlistTrackIDs = nil },
                onAdded: finishAddingToPlaylist
            )
        }
        .onChange(of: model.tracks.map(\.id)) { _, _ in reconcileSelection() }
        .onChange(of: model.phase) { _, phase in
            guard phase == .results else {
                selectedTrackIDs.removeAll()
                playlistTrackIDs = nil
                actionMessage = nil
                return
            }
            reconcileSelection()
        }
    }

    private var emptyState: some View {
        let presentation = LibrarySearchEmptyPresentation(query: model.query)
        return ContentUnavailableView {
            Label("没有搜索结果", systemImage: "magnifyingglass")
        } description: {
            Text(presentation.message)
        } actions: {
            Button(presentation.clearActionTitle, action: model.clear)
        }
    }

    private var results: some View {
        VStack(spacing: 0) {
            if let actionMessage {
                HStack(spacing: 8) {
                    Image(systemName: "checkmark.circle.fill").foregroundStyle(.green)
                    Text(actionMessage)
                    Spacer()
                    Button("关闭") { self.actionMessage = nil }
                }
                .font(.caption)
                .padding(.horizontal, 16)
                .padding(.vertical, 7)
                .background(.bar)
            }

            List(selection: $selectedTrackIDs) {
                if !model.artists.isEmpty {
                    Section("艺人") { horizontalArtists }
                    pagination(category: .artists, hasMore: model.hasMoreArtists)
                }
                if !model.albums.isEmpty {
                    Section("专辑") { horizontalAlbums }
                    pagination(category: .albums, hasMore: model.hasMoreAlbums)
                }
                if !model.tracks.isEmpty {
                    Section("曲目") {
                        ForEach(Array(model.tracks.enumerated()), id: \.element.id) { index, track in
                            searchTrackRow(track, index: index)
                                .tag(track.id)
                        }
                    }
                    pagination(category: .tracks, hasMore: model.hasMoreTracks)
                }
            }
            .listStyle(.inset)
            .focusable()
            .onKeyPress(.return) {
                guard let first = selectedTracks.first,
                      let index = model.tracks.firstIndex(where: { $0.id == first.id })
                else { return .ignored }
                playTracks(startingAt: index)
                return .handled
            }
            .onKeyPress(phases: .down) { keyPress in
                guard keyPress.key == "a", keyPress.modifiers.contains(.command) else {
                    return .ignored
                }
                selectedTrackIDs = SearchTrackWorkbenchPolicy.selectAll(model.tracks)
                return .handled
            }

            if !selectedTrackIDs.isEmpty { batchAccessory }
        }
    }

    private var horizontalArtists: some View {
        ScrollView(.horizontal) {
            LazyHStack(spacing: 12) {
                ForEach(model.artists, id: \.id) { artist in
                    VStack(alignment: .leading) {
                        Circle().fill(.quaternary).frame(width: 68, height: 68).overlay {
                            Text(String(artist.name.prefix(1)).uppercased()).font(.title2.bold())
                        }
                        Text(artist.name).lineLimit(2)
                    }
                    .frame(width: 100, alignment: .leading)
                    .padding(6)
                    .background(
                        selection.isArtistHighlighted(artist.id) ? Color.accentColor.opacity(0.16) : .clear,
                        in: RoundedRectangle(cornerRadius: 7)
                    )
                    .contentShape(Rectangle())
                    .focusable()
                    .onTapGesture(count: 2) { onOpenArtist(artist) }
                    .onTapGesture { onHighlightArtist(artist) }
                    .onKeyPress(.return) {
                        onOpenArtist(artist)
                        return .handled
                    }
                    .accessibilityLabel("艺人 \(artist.name)，\(artist.albumCount) 张专辑")
                    .accessibilityAction(named: "打开艺人") { onOpenArtist(artist) }
                }
            }
        }
    }

    private var horizontalAlbums: some View {
        ScrollView(.horizontal) {
            LazyHStack(spacing: 14) {
                ForEach(model.albums, id: \.id) { album in
                    SearchAlbumCell(
                        album: album,
                        client: client,
                        isHighlighted: selection.isAlbumHighlighted(album.id),
                        onHighlight: onHighlightAlbum,
                        onOpen: onOpenAlbum
                    )
                }
            }
        }
    }

    private func searchTrackRow(_ track: Track, index: Int) -> some View {
        HStack(spacing: 10) {
            Button { playTracks(startingAt: index) } label: {
                Image(systemName: "play.fill").frame(width: 18)
            }
            .buttonStyle(.plain)
            .accessibilityLabel("播放 \(track.title)")
            VStack(alignment: .leading, spacing: 2) {
                Text(track.title)
                Text(track.artist ?? "未知艺人").font(.caption).foregroundStyle(.secondary)
            }
            Spacer()
            if let album = track.album, !album.isEmpty {
                Text(album)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .frame(maxWidth: 180, alignment: .leading)
            }
            Text(playbackTime(track.duration)).font(.caption.monospacedDigit()).foregroundStyle(.tertiary)
        }
        .padding(.vertical, 7)
        .contentShape(Rectangle())
        .contextMenu {
            PlaybackTrackActions(track: track, playback: playback)
            Divider()
            Button("加入歌单…") { playlistTrackIDs = [track.id] }
        }
        .accessibilityLabel("曲目 \(track.title)，艺人 \(track.artist ?? "未知")")
        .accessibilityAction(named: "播放") { playTracks(startingAt: index) }
    }

    private var batchAccessory: some View {
        HStack(spacing: 12) {
            Text("已选择 \(selectedTrackIDs.count) 首")
                .font(.headline)
            Spacer()
            Button("播放所选") { playSelectedTracks() }
            Button("加入歌单…") { playlistTrackIDs = selectedTracks.map(\.id) }
                .disabled(playlists.isMutating)
            Button("清除选择") { selectedTrackIDs.removeAll() }
        }
        .padding(.horizontal, 18)
        .frame(height: 52)
        .background(.bar)
        .overlay(alignment: .top) { Divider() }
    }

    @ViewBuilder private func pagination(category: SearchResultCategory, hasMore: Bool) -> some View {
        if model.isLoading(category) {
            ProgressView("正在加载更多…")
                .controlSize(.small)
        } else if let error = model.nextPageErrors[category] {
            HStack {
                Text(error).font(.caption).foregroundStyle(.red)
                Button("重试") { Task { await model.loadMore(category) } }
                    .disabled(model.isLoading(category))
            }
        } else if hasMore {
            Button("加载更多") { Task { await model.loadMore(category) } }
                .disabled(model.isLoading(category))
        }
    }

    private func playTracks(startingAt index: Int) {
        Task { await playback.play(tracks: model.tracks, startingAt: index) }
    }

    private var selectedTracks: [Track] {
        SearchTrackWorkbenchPolicy.selectedTracks(model.tracks, selection: selectedTrackIDs)
    }

    private var playlistPickerIsPresented: Binding<Bool> {
        Binding(get: { playlistTrackIDs != nil }, set: { if !$0 { playlistTrackIDs = nil } })
    }

    private func playSelectedTracks() {
        let tracks = selectedTracks
        guard !tracks.isEmpty else { return }
        Task { await playback.play(tracks: tracks, startingAt: 0) }
    }

    private func reconcileSelection() {
        selectedTrackIDs = SearchTrackWorkbenchPolicy.reconciledSelection(
            selectedTrackIDs,
            tracks: model.tracks
        )
    }

    private func finishAddingToPlaylist() {
        let count = playlistTrackIDs?.count ?? 0
        playlistTrackIDs = nil
        selectedTrackIDs.removeAll()
        actionMessage = count == 1 ? "已加入歌单" : "已将 \(count) 首歌曲加入歌单"
    }
}

private struct SearchAlbumCell: View {
    let album: Album
    let client: any MusicClientProviding
    let isHighlighted: Bool
    let onHighlight: (Album) -> Void
    let onOpen: (Album) -> Void
    @State private var coverURL: URL?

    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            AuthenticatedArtworkView(url: coverURL) { Rectangle().fill(.quaternary) }
                .frame(width: 116, height: 116)
                .clipped()
                .clipShape(RoundedRectangle(cornerRadius: 7))
            Text(album.name).lineLimit(2)
            Text(album.artist ?? "未知艺人").font(.caption).foregroundStyle(.secondary).lineLimit(1)
        }
        .frame(width: 116, alignment: .leading)
        .padding(6)
        .overlay {
            RoundedRectangle(cornerRadius: 8)
                .stroke(isHighlighted ? Color.accentColor : .clear, lineWidth: 2)
        }
        .contentShape(Rectangle())
        .focusable()
        .onTapGesture(count: 2) { onOpen(album) }
        .onTapGesture { onHighlight(album) }
        .onKeyPress(.return) {
            onOpen(album)
            return .handled
        }
        .accessibilityLabel("专辑 \(album.name)，艺人 \(album.artist ?? "未知")")
        .accessibilityAction(named: "打开专辑") { onOpen(album) }
        .task(id: album.coverArt) { coverURL = await loadCoverURL(for: album, client: client) }
    }
}
