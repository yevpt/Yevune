import SwiftUI
import YevuneCoreFFI

struct AlbumCollectionView: View {
    let albums: [Album]
    let style: LibraryCollectionStyle
    let client: any MusicClientProviding
    let isAdmin: Bool
    let hasMoreAlbums: Bool
    let isLoadingNextPage: Bool
    let nextPageError: String?
    let onSelect: (Album) -> Void
    let onLoadNextPage: () async -> Void

    var body: some View {
        Group {
            if albums.isEmpty, !isLoadingNextPage {
                ContentUnavailableView {
                    Label(LibraryPresentation.emptyLibraryMessage(isAdmin: isAdmin), systemImage: "opticaldisc")
                }
            } else if style == .grid {
                ScrollView {
                    LazyVGrid(
                        columns: [GridItem(.adaptive(minimum: 156, maximum: 190), spacing: 18)],
                        spacing: 22
                    ) {
                        ForEach(albums, id: \.id) { album in
                            AlbumCollectionCell(album: album, client: client, onSelect: onSelect)
                                .onAppear { loadIfLast(album) }
                        }
                    }
                    .padding(18)
                    paginationFooter
                }
            } else {
                List(albums, id: \.id) { album in
                    Button { onSelect(album) } label: {
                        VStack(alignment: .leading, spacing: 3) {
                            Text(album.name).lineLimit(2)
                            Text(album.artist ?? "未知艺人")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    .buttonStyle(.plain)
                    .onAppear { loadIfLast(album) }
                }
                .safeAreaInset(edge: .bottom) { paginationFooter }
            }
        }
    }

    @ViewBuilder private var paginationFooter: some View {
        if isLoadingNextPage {
            ProgressView("正在加载更多专辑…").padding()
        } else if let nextPageError {
            HStack {
                Text(nextPageError).foregroundStyle(.red).lineLimit(2)
                Button("重试") { Task { await onLoadNextPage() } }
            }
            .padding()
        }
    }

    private func loadIfLast(_ album: Album) {
        guard hasMoreAlbums, album.id == albums.last?.id else { return }
        Task { await onLoadNextPage() }
    }
}

private struct AlbumCollectionCell: View {
    let album: Album
    let client: any MusicClientProviding
    let onSelect: (Album) -> Void
    @State private var coverURL: URL?

    var body: some View {
        Button { onSelect(album) } label: {
            VStack(alignment: .leading, spacing: 7) {
                AuthenticatedArtworkView(url: coverURL) {
                    Rectangle().fill(.quaternary)
                        .overlay { Image(systemName: "opticaldisc").foregroundStyle(.secondary) }
                }
                .aspectRatio(1, contentMode: .fit)
                .clipShape(RoundedRectangle(cornerRadius: 8))

                Text(album.name)
                    .font(.subheadline.weight(.semibold))
                    .lineLimit(2)
                    .multilineTextAlignment(.leading)
                Text(album.artist ?? "未知艺人")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .buttonStyle(.plain)
        .accessibilityLabel("专辑 \(album.name)，艺人 \(album.artist ?? "未知")")
        .task(id: album.coverArt) {
            coverURL = await loadCoverURL(for: album, client: client)
        }
    }
}
