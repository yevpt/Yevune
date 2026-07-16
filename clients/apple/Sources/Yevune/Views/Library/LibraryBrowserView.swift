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
    @State private var selection: LibraryNavigationSelection?
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
        Binding(
            get: { selection.map { [$0] } ?? [] },
            set: { selection = $0.last }
        )
    }

    private var preservedCollection: some View {
        ZStack {
            collection
                .opacity(search.query.isEmpty ? 1 : 0)
                .allowsHitTesting(search.query.isEmpty)

            if !search.query.isEmpty {
                LibrarySearchResultsView(
                    model: search,
                    playback: playback,
                    client: client,
                    onSelectArtist: { selection = .artist($0.id) },
                    onSelectAlbum: { selection = .album($0.id) }
                )
            }
        }
    }

    @ViewBuilder private var collection: some View {
        if browse.section == .albums {
            AlbumCollectionView(
                albums: browse.albums,
                style: collectionStyle,
                client: client,
                isAdmin: session.admin,
                hasMoreAlbums: browse.hasMoreAlbums,
                isLoadingNextPage: browse.isLoadingNextPage,
                nextPageError: browse.nextPageError,
                onSelect: { selection = .album($0.id) },
                onLoadNextPage: browse.loadNextPage
            )
        } else {
            ArtistCollectionView(
                artists: browse.artists,
                client: client,
                isAdmin: session.admin,
                onSelect: { selection = .artist($0.id) }
            )
        }
    }

    @ViewBuilder private var inspector: some View {
        if let selection {
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
                onSelectAlbum: { selection = .album($0.id) },
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
        LibraryNavigationAction.returnToLibrary.apply(to: &selection)
    }
}
