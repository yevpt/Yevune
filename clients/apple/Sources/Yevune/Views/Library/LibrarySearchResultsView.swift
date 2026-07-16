import SwiftUI
import YevuneCoreFFI

struct LibrarySearchResultsView: View {
    @ObservedObject var model: LibrarySearchViewModel
    @ObservedObject var playback: PlaybackController
    let client: any MusicClientProviding
    let onSelectArtist: (Artist) -> Void
    let onSelectAlbum: (Album) -> Void

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
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 22) {
                if !model.artists.isEmpty {
                    resultSection("艺人") {
                        horizontalArtists
                    }
                    pagination(category: .artists, hasMore: model.hasMoreArtists)
                }
                if !model.albums.isEmpty {
                    resultSection("专辑") {
                        horizontalAlbums
                    }
                    pagination(category: .albums, hasMore: model.hasMoreAlbums)
                }
                if !model.tracks.isEmpty {
                    resultSection("曲目") {
                        VStack(spacing: 0) {
                            ForEach(Array(model.tracks.enumerated()), id: \.element.id) { index, track in
                                searchTrackRow(track, index: index)
                                Divider()
                            }
                        }
                    }
                    pagination(category: .tracks, hasMore: model.hasMoreTracks)
                }
            }
            .padding(18)
        }
        .background(.background)
    }

    private func resultSection<Content: View>(_ title: String, @ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(title).font(.title2.bold())
            content()
        }
    }

    private var horizontalArtists: some View {
        ScrollView(.horizontal) {
            LazyHStack(spacing: 12) {
                ForEach(model.artists, id: \.id) { artist in
                    Button { onSelectArtist(artist) } label: {
                        VStack(alignment: .leading) {
                            Circle().fill(.quaternary).frame(width: 68, height: 68).overlay {
                                Text(String(artist.name.prefix(1)).uppercased()).font(.title2.bold())
                            }
                            Text(artist.name).lineLimit(2)
                        }
                        .frame(width: 100, alignment: .leading)
                    }
                    .buttonStyle(.plain)
                }
            }
        }
    }

    private var horizontalAlbums: some View {
        ScrollView(.horizontal) {
            LazyHStack(spacing: 14) {
                ForEach(model.albums, id: \.id) { album in
                    SearchAlbumCell(album: album, client: client, onSelect: onSelectAlbum)
                }
            }
        }
    }

    private func searchTrackRow(_ track: Track, index: Int) -> some View {
        HStack(spacing: 10) {
            Image(systemName: "music.note").foregroundStyle(.secondary).frame(width: 18)
            VStack(alignment: .leading, spacing: 2) {
                Text(track.title)
                Text(track.artist ?? "未知艺人").font(.caption).foregroundStyle(.secondary)
            }
            Spacer()
            Text(playbackTime(track.duration)).font(.caption.monospacedDigit()).foregroundStyle(.tertiary)
        }
        .padding(.vertical, 7)
        .contentShape(Rectangle())
        .focusable()
        .onTapGesture(count: 2) { playTracks(startingAt: index) }
        .onKeyPress(.return) {
            playTracks(startingAt: index)
            return .handled
        }
        .accessibilityAction(named: "播放") { playTracks(startingAt: index) }
    }

    @ViewBuilder private func pagination(category: SearchResultCategory, hasMore: Bool) -> some View {
        if let error = model.nextPageErrors[category] {
            HStack {
                Text(error).font(.caption).foregroundStyle(.red)
                Button("重试") { Task { await model.loadMore(category) } }
            }
        } else if hasMore {
            Button("加载更多") { Task { await model.loadMore(category) } }
        }
    }

    private func playTracks(startingAt index: Int) {
        Task { await playback.play(tracks: model.tracks, startingAt: index) }
    }
}

private struct SearchAlbumCell: View {
    let album: Album
    let client: any MusicClientProviding
    let onSelect: (Album) -> Void
    @State private var coverURL: URL?

    var body: some View {
        Button { onSelect(album) } label: {
            VStack(alignment: .leading, spacing: 5) {
                AuthenticatedArtworkView(url: coverURL) { Rectangle().fill(.quaternary) }
                    .frame(width: 116, height: 116)
                    .clipped()
                    .clipShape(RoundedRectangle(cornerRadius: 7))
                Text(album.name).lineLimit(2)
                Text(album.artist ?? "未知艺人").font(.caption).foregroundStyle(.secondary).lineLimit(1)
            }
            .frame(width: 116, alignment: .leading)
        }
        .buttonStyle(.plain)
        .task(id: album.coverArt) { coverURL = await loadCoverURL(for: album, client: client) }
    }
}
