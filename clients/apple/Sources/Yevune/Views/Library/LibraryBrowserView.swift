import SwiftUI
import YevuneCoreFFI

struct LibraryBrowserView: View {
    @ObservedObject var browse: LibraryBrowseViewModel
    @ObservedObject var search: LibrarySearchViewModel
    @ObservedObject var artistDetail: ArtistDetailViewModel
    @ObservedObject var playback: PlaybackController
    let client: any MusicClientProviding
    let session: SessionValue
    let onImportMusic: () -> Void
    let onScanLibrary: () -> Void
    let onShowTasks: () -> Void

    @StateObject private var media: MediaViewModel
    @StateObject private var playlists: PlaylistViewModel
    @State private var navigation = LibraryNavigationState()
    @State private var collectionStyle = LibraryCollectionStyle.grid

    init(
        browse: LibraryBrowseViewModel,
        search: LibrarySearchViewModel,
        artistDetail: ArtistDetailViewModel,
        client: any MusicClientProviding,
        playback: PlaybackController,
        session: SessionValue,
        onImportMusic: @escaping () -> Void = {},
        onScanLibrary: @escaping () -> Void = {},
        onShowTasks: @escaping () -> Void = {}
    ) {
        self.browse = browse
        self.search = search
        self.artistDetail = artistDetail
        self.client = client
        self.playback = playback
        self.session = session
        self.onImportMusic = onImportMusic
        self.onScanLibrary = onScanLibrary
        self.onShowTasks = onShowTasks
        _media = StateObject(wrappedValue: MediaViewModel(client: client))
        _playlists = StateObject(wrappedValue: PlaylistViewModel(client: client))
    }

    var body: some View {
        GeometryReader { geometry in
            let presentation = LibraryPresentation(width: geometry.size.width, isAdmin: session.admin)
            VStack(spacing: 0) {
                LibraryCommandBar(
                    browse: browse,
                    search: search,
                    presentation: presentation,
                    collectionStyle: $collectionStyle,
                    onImportMusic: onImportMusic,
                    onScanLibrary: onScanLibrary,
                    onShowTasks: onShowTasks
                )
                Divider()
                if presentation.layout == .regular {
                    regularLayout
                } else {
                    compactLayout
                }
            }
        }
        .task {
            async let browseLoad: Void = browse.reload()
            async let playlistLoad: Void = playlists.loadTree()
            _ = await (browseLoad, playlistLoad)
        }
        .onExitCommand(perform: handleEscape)
        .onChange(of: search.query) { _, _ in reconcileNavigation(isQueryChange: true) }
        .onChange(of: browse.section) { _, _ in reconcileNavigation() }
        .onChange(of: browse.albumCriterion) { _, _ in reconcileNavigation() }
        .onChange(of: browse.albums.map(\.id)) { _, _ in reconcileNavigation() }
        .onChange(of: browse.artists.map(\.id)) { _, _ in reconcileNavigation() }
        .onChange(of: search.albums.map(\.id)) { _, _ in reconcileNavigation() }
        .onChange(of: search.artists.map(\.id)) { _, _ in reconcileNavigation() }
    }

    private var regularLayout: some View {
        HStack(spacing: 0) {
            preservedCollection
            Divider()
            inspector
                .frame(minWidth: 380, idealWidth: 420, maxWidth: 480)
        }
    }

    private var compactLayout: some View {
        NavigationStack(path: $navigation.path) {
            preservedCollection
                .navigationDestination(for: LibraryNavigationSelection.self) { destination in
                    destinationView(destination)
                }
        }
    }

    private var preservedCollection: some View {
        ZStack {
            browseContent
                .opacity(search.query.isEmpty ? 1 : 0)
                .allowsHitTesting(search.query.isEmpty)

            if !search.query.isEmpty {
                LibrarySearchResultsView(
                    model: search,
                    playback: playback,
                    client: client,
                    onSelectArtist: { navigation.openArtist(id: $0.id) },
                    onSelectAlbum: { navigation.openAlbum(id: $0.id) }
                )
            }
        }
    }

    @ViewBuilder private var browseContent: some View {
        switch browsePresentation {
        case .loading:
            ProgressView("正在加载曲库…")
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        case .initialFailure(let message):
            ContentUnavailableView {
                Label("无法加载曲库", systemImage: "wifi.exclamationmark")
            } description: {
                Text(message)
            } actions: {
                Button("重试") { Task { await browse.reload() } }
            }
        case .empty(let message):
            ContentUnavailableView {
                Label(message, systemImage: browse.section == .albums ? "opticaldisc" : "person.2")
            }
        case .content(let isRefreshing, let refreshError):
            VStack(spacing: 0) {
                if isRefreshing {
                    ProgressView().controlSize(.small).padding(.vertical, 5)
                }
                if let refreshError {
                    HStack(spacing: 10) {
                        Image(systemName: "exclamationmark.triangle")
                        Text(refreshError).lineLimit(2)
                        Spacer()
                        Button("重试") { Task { await browse.reload() } }
                    }
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 7)
                    Divider()
                }
                collection
            }
        }
    }

    private var browsePresentation: LibraryBrowsePresentation {
        LibraryBrowsePresentation.resolve(
            contentCount: browse.section == .albums ? browse.albums.count : browse.artists.count,
            isRefreshing: browse.isRefreshing,
            initialError: browse.initialError,
            refreshError: browse.refreshError,
            isAdmin: session.admin
        )
    }

    @ViewBuilder private var collection: some View {
        if browse.section == .albums {
            AlbumCollectionView(
                albums: browse.albums,
                selectedAlbumID: navigation.selectedAlbumID,
                style: collectionStyle,
                client: client,
                isAdmin: session.admin,
                hasMoreAlbums: browse.hasMoreAlbums,
                isLoadingNextPage: browse.isLoadingNextPage,
                nextPageError: browse.nextPageError,
                onSelect: { navigation.openAlbum(id: $0.id) },
                onLoadNextPage: browse.loadNextPage
            )
        } else {
            ArtistCollectionView(
                artists: browse.artists,
                client: client,
                isAdmin: session.admin,
                onSelect: { navigation.openArtist(id: $0.id) }
            )
        }
    }

    @ViewBuilder private var inspector: some View {
        if let selection = navigation.path.last {
            destinationView(selection)
        } else {
            ContentUnavailableView("选择专辑或艺人", systemImage: "music.note.list")
        }
    }

    @ViewBuilder private func destinationView(_ destination: LibraryNavigationSelection) -> some View {
        switch destination {
        case .artist(let id):
            ArtistDetailView(
                model: artistDetail,
                artistID: id,
                client: client,
                isAdmin: session.admin,
                onSelectAlbum: { navigation.openAlbum(id: $0.id) },
                onReturn: returnToLibrary
            )
        case .album(let id):
            if let album = album(id: id) {
                MediaDetailView(album: album, model: media, playlists: playlists, playback: playback)
                    .navigationTitle("返回曲库，继续播放")
                    .toolbar { Button("返回曲库，继续播放", action: returnToLibrary) }
            } else {
                ContentUnavailableView("无法找到专辑", systemImage: "opticaldisc")
            }
        }
    }

    private func album(id: String) -> Album? {
        browse.albums.first { $0.id == id }
            ?? search.albums.first { $0.id == id }
            ?? artistDetail.detail?.albums.first { $0.id == id }
    }

    private func returnToLibrary() {
        navigation.returnToLibrary()
    }

    private func handleEscape() {
        switch navigation.handleEscape(isSearchActive: !search.query.isEmpty) {
        case .clearSearch: search.clear()
        case .closeNavigation, .ignored: break
        }
    }

    private func reconcileNavigation(isQueryChange: Bool = false) {
        let visibleAlbums: [String]
        let visibleArtists: [String]
        if search.query.isEmpty {
            visibleAlbums = browse.section == .albums ? browse.albums.map(\.id) : []
            visibleArtists = browse.section == .artists ? browse.artists.map(\.id) : []
        } else {
            visibleAlbums = search.albums.map(\.id)
            visibleArtists = search.artists.map(\.id)
        }
        if isQueryChange {
            navigation.reconcileForQueryChange(
                visibleAlbumIDs: Set(visibleAlbums),
                visibleArtistIDs: Set(visibleArtists)
            )
        } else {
            navigation.reconcile(
                visibleAlbumIDs: Set(visibleAlbums),
                visibleArtistIDs: Set(visibleArtists)
            )
        }
    }
}
