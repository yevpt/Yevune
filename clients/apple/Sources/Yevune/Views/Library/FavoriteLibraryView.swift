import SwiftUI
import YevuneCoreFFI

struct FavoriteLibraryView: View {
    @ObservedObject var model: FavoriteLibraryViewModel
    @ObservedObject var artistDetail: ArtistDetailViewModel
    @ObservedObject var playback: PlaybackController
    @ObservedObject var playlists: PlaylistViewModel
    let client: any MusicClientProviding
    let isAdmin: Bool
    let onImportMusic: () -> Void
    let onManageAccess: (AccessScopeTarget) -> Void

    @StateObject private var media: MediaViewModel
    @State private var path: [LibraryNavigationSelection] = []
    @State private var selectedTrackID: String?
    @State private var playlistTrackIDs: [String]?

    init(
        model: FavoriteLibraryViewModel,
        artistDetail: ArtistDetailViewModel,
        playback: PlaybackController,
        playlists: PlaylistViewModel,
        client: any MusicClientProviding,
        isAdmin: Bool,
        onImportMusic: @escaping () -> Void = {},
        onManageAccess: @escaping (AccessScopeTarget) -> Void = { _ in }
    ) {
        self.model = model
        self.artistDetail = artistDetail
        self.playback = playback
        self.playlists = playlists
        self.client = client
        self.isAdmin = isAdmin
        self.onImportMusic = onImportMusic
        self.onManageAccess = onManageAccess
        _media = StateObject(wrappedValue: MediaViewModel(client: client))
    }

    var body: some View {
        NavigationStack(path: $path) {
            content
                .navigationDestination(for: LibraryNavigationSelection.self) { destination in
                    destinationView(destination)
                }
        }
        .task { await model.load() }
        .sheet(isPresented: playlistPickerIsPresented) {
            PlaylistPickerSheet(
                playlists: playlists,
                trackIDs: playlistTrackIDs ?? [],
                onCancel: { playlistTrackIDs = nil },
                onAdded: { playlistTrackIDs = nil }
            )
        }
    }

    private var content: some View {
        VStack(spacing: 0) {
            header
            Divider()
            if model.isLoading, model.isEmpty {
                ProgressView("正在加载收藏…")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if let message = model.initialError, model.isEmpty {
                ContentUnavailableView {
                    Label("无法加载收藏", systemImage: "wifi.exclamationmark")
                } description: {
                    Text(message)
                } actions: {
                    Button("重试") { Task { await model.refresh() } }
                }
            } else {
                sectionContent
            }
        }
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(alignment: .firstTextBaseline) {
                VStack(alignment: .leading, spacing: 3) {
                    Text("收藏").font(.largeTitle.bold())
                    Text("\(model.tracks.count) 首歌曲 · \(model.albums.count) 张专辑 · \(model.artists.count) 位艺人")
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Button {
                    Task { await model.refresh() }
                } label: {
                    Label("刷新收藏", systemImage: "arrow.clockwise")
                }
                .disabled(model.isLoading)
            }
            Picker("收藏类型", selection: $model.section) {
                ForEach(FavoriteLibrarySection.allCases) { section in
                    Text(section.rawValue).tag(section)
                }
            }
            .pickerStyle(.segmented)
            .frame(maxWidth: 420)
            if let message = model.refreshError {
                Label(message, systemImage: "exclamationmark.triangle")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 16)
    }

    @ViewBuilder private var sectionContent: some View {
        switch model.section {
        case .tracks:
            if model.tracks.isEmpty { emptySection("还没有收藏歌曲", icon: "heart") }
            else { trackList }
        case .albums:
            if model.albums.isEmpty { emptySection("还没有收藏专辑", icon: "heart") }
            else {
                AlbumCollectionView(
                    albums: model.albums,
                    highlightedAlbumID: nil,
                    style: .grid,
                    client: client,
                    isAdmin: isAdmin,
                    hasMoreAlbums: false,
                    isLoadingNextPage: false,
                    nextPageError: nil,
                    onHighlight: { _ in },
                    onOpen: { path = [.album($0.id)] },
                    onLoadNextPage: {},
                    onStarredChanged: { album, starred in
                        if !starred { model.remove(.album(album.id)) }
                    }
                )
            }
        case .artists:
            if model.artists.isEmpty { emptySection("还没有收藏艺人", icon: "heart") }
            else {
                ArtistCollectionView(
                    artists: model.artists,
                    highlightedArtistID: nil,
                    client: client,
                    isAdmin: isAdmin,
                    onHighlight: { _ in },
                    onOpen: { path = [.artist($0.id)] },
                    onStarredChanged: { artist, starred in
                        if !starred { model.remove(.artist(artist.id)) }
                    }
                )
            }
        }
    }

    private var trackList: some View {
        List(selection: $selectedTrackID) {
            ForEach(Array(model.tracks.enumerated()), id: \.element.id) { index, track in
                favoriteTrackRow(track, index: index).tag(track.id)
            }
        }
        .listStyle(.inset)
        .onKeyPress(.return) {
            guard let selectedTrackID,
                  let index = model.tracks.firstIndex(where: { $0.id == selectedTrackID })
            else { return .ignored }
            play(startingAt: index)
            return .handled
        }
    }

    private func favoriteTrackRow(_ track: Track, index: Int) -> some View {
        HStack(spacing: 12) {
            Button { play(startingAt: index) } label: {
                Image(systemName: "play.fill").frame(width: 20)
            }
            .buttonStyle(.plain)
            .accessibilityLabel("播放 \(track.title)")
            VStack(alignment: .leading, spacing: 2) {
                Text(track.title).font(.body.weight(.medium)).lineLimit(1)
                Text(track.artist ?? "未知艺人")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer()
            MediaAnnotationIndicator(
                target: .track(track.id),
                starred: track.starred,
                rating: track.userRating
            )
            Text(track.album ?? "未知专辑")
                .font(.caption)
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .frame(maxWidth: 220, alignment: .leading)
            Text(playbackTime(track.duration))
                .font(.caption.monospacedDigit())
                .foregroundStyle(.tertiary)
        }
        .padding(.vertical, 7)
        .contentShape(Rectangle())
        .onTapGesture(count: 2) { play(startingAt: index) }
        .contextMenu {
            PlaybackTrackActions(track: track, playback: playback)
            Divider()
            Button("加入歌单…") { playlistTrackIDs = [track.id] }
            Divider()
            MediaAnnotationMenuActions(
                target: .track(track.id),
                starred: track.starred,
                rating: track.userRating,
                onStarredChanged: { starred in
                    if !starred { model.remove(.track(track.id)) }
                }
            )
        }
        .accessibilityLabel("收藏歌曲 \(track.title)，艺人 \(track.artist ?? "未知")")
        .accessibilityAction(named: "播放") { play(startingAt: index) }
    }

    @ViewBuilder private func destinationView(_ destination: LibraryNavigationSelection) -> some View {
        switch destination {
        case .album(let id):
            if let album = album(id) {
                MediaDetailView(
                    album: album,
                    model: media,
                    playlists: playlists,
                    playback: playback,
                    isAdmin: isAdmin,
                    onImportMusic: onImportMusic,
                    onManageAccess: isAdmin ? onManageAccess : nil
                )
                .navigationTitle("返回收藏，继续播放")
            } else {
                ContentUnavailableView("无法找到专辑", systemImage: "opticaldisc")
            }
        case .artist(let id):
            ArtistDetailView(
                model: artistDetail,
                artistID: id,
                client: client,
                isAdmin: isAdmin,
                onSelectAlbum: { path.append(.album($0.id)) },
                onReturn: { path.removeAll() },
                returnTitle: "返回收藏，继续播放"
            )
        }
    }

    private func album(_ id: String) -> Album? {
        model.albums.first { $0.id == id }
            ?? artistDetail.detail?.albums.first { $0.id == id }
    }

    private func play(startingAt index: Int) {
        Task { await playback.play(tracks: model.tracks, startingAt: index) }
    }

    private func emptySection(_ title: String, icon: String) -> some View {
        ContentUnavailableView {
            Label(title, systemImage: icon)
        } description: {
            Text("在曲库、搜索或播放页点击收藏后，会出现在这里。")
        }
    }

    private var playlistPickerIsPresented: Binding<Bool> {
        Binding(get: { playlistTrackIDs != nil }, set: { if !$0 { playlistTrackIDs = nil } })
    }
}
