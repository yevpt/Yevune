import SwiftUI
import YevuneCoreFFI

struct AlbumCollectionView: View {
    let albums: [Album]
    let selectedAlbumID: String?
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
                            AlbumCollectionCell(
                                album: album,
                                client: client,
                                isSelected: selectedAlbumID == album.id,
                                onSelect: select
                            )
                                .onAppear { loadIfLast(album) }
                        }
                    }
                    .padding(18)
                    paginationFooter
                }
            } else {
                List(albums, id: \.id) { album in
                    Button { select(album) } label: {
                        VStack(alignment: .leading, spacing: 3) {
                            Text(album.name).lineLimit(2)
                            Text(album.artist ?? "未知艺人")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(6)
                        .background(
                            selectedAlbumID == album.id ? Color.accentColor.opacity(0.16) : .clear,
                            in: RoundedRectangle(cornerRadius: 6)
                        )
                    }
                    .buttonStyle(.plain)
                    .onTapGesture(count: 2) { select(album) }
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

    private func select(_ album: Album) {
        onSelect(album)
    }
}

private struct AlbumCollectionCell: View {
    let album: Album
    let client: any MusicClientProviding
    let isSelected: Bool
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
        .overlay {
            RoundedRectangle(cornerRadius: 9)
                .stroke(isSelected ? Color.accentColor : .clear, lineWidth: 2)
                .padding(-4)
        }
        .onTapGesture(count: 2) { onSelect(album) }
        .accessibilityLabel("专辑 \(album.name)，艺人 \(album.artist ?? "未知")")
        .task(id: album.coverArt) {
            coverURL = await loadCoverURL(for: album, client: client)
        }
    }
}
