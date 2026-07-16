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
        .onChange(of: search.phase) { _, _ in reconcileSearchNavigation() }
        .onChange(of: browse.section) { _, _ in reconcileExplicitBrowseChange() }
        .onChange(of: browse.albumCriterion) { _, _ in reconcileExplicitBrowseChange() }
        .onChange(of: browse.albums.map(\.id)) { _, _ in reconcileBrowseNavigation() }
        .onChange(of: browse.artists.map(\.id)) { _, _ in reconcileBrowseNavigation() }
        .onChange(of: search.albums.map(\.id)) { _, _ in reconcileSearchNavigation() }
        .onChange(of: search.artists.map(\.id)) { _, _ in reconcileSearchNavigation() }
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
        NavigationStack(path: compactPath) {
            preservedCollection
                .navigationDestination(for: LibraryNavigationSelection.self) { destination in
                    destinationView(destination)
                }
        }
    }

    private var compactPath: Binding<[LibraryNavigationSelection]> {
        Binding(get: { navigation.path }, set: { navigation.setPath($0) })
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
                    highlightedAlbumID: navigation.highlightedAlbumID,
                    highlightedArtistID: navigation.highlightedArtistID,
                    onHighlightArtist: { navigation.highlightArtist(id: $0.id) },
                    onOpenArtist: { navigation.openArtist(id: $0.id) },
                    onHighlightAlbum: { navigation.highlightAlbum(id: $0.id) },
                    onOpenAlbum: { navigation.openAlbum($0) }
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
                highlightedAlbumID: navigation.highlightedAlbumID,
                style: collectionStyle,
                client: client,
                isAdmin: session.admin,
                hasMoreAlbums: browse.hasMoreAlbums,
                isLoadingNextPage: browse.isLoadingNextPage,
                nextPageError: browse.nextPageError,
                onHighlight: { navigation.highlightAlbum(id: $0.id) },
                onOpen: { navigation.openAlbum($0) },
                onLoadNextPage: browse.loadNextPage
            )
        } else {
            ArtistCollectionView(
                artists: browse.artists,
                highlightedArtistID: navigation.highlightedArtistID,
                client: client,
                isAdmin: session.admin,
                onHighlight: { navigation.highlightArtist(id: $0.id) },
                onOpen: { navigation.openArtist(id: $0.id) }
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
                onSelectAlbum: { navigation.openAlbum($0) },
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
            ?? navigation.routedAlbumSnapshot.flatMap { $0.id == id ? $0 : nil }
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

    private func reconcileSearchNavigation() {
        navigation.reconcileSearch(
            phase: search.phase,
            searchAlbumIDs: Set(search.albums.map(\.id)),
            searchArtistIDs: Set(search.artists.map(\.id)),
            browseAlbumIDs: Set(browse.section == .albums ? browse.albums.map(\.id) : []),
            browseArtistIDs: Set(browse.section == .artists ? browse.artists.map(\.id) : [])
        )
    }

    private func reconcileBrowseNavigation() {
        guard search.phase == .idle else { return }
        navigation.reconcileBrowse(
            visibleAlbumIDs: Set(browse.section == .albums ? browse.albums.map(\.id) : []),
            visibleArtistIDs: Set(browse.section == .artists ? browse.artists.map(\.id) : [])
        )
    }

    private func reconcileExplicitBrowseChange() {
        guard search.phase == .idle else { return }
        navigation.resumeBrowseReconciliation()
        reconcileBrowseNavigation()
    }
}
