import SwiftUI
import YevuneCoreFFI

struct ArtistDetailView: View {
    @ObservedObject var model: ArtistDetailViewModel
    let artistID: String
    let client: any MusicClientProviding
    let isAdmin: Bool
    let onSelectAlbum: (Album) -> Void
    let onReturn: () -> Void

    var body: some View {
        Group {
            if let detail = model.detail, detail.artist.id == artistID {
                VStack(alignment: .leading, spacing: 12) {
                    HStack(spacing: 12) {
                        Text(detail.artist.name)
                            .font(.largeTitle.bold())
                        Spacer()
                        MediaFavoriteButton(
                            target: .artist(detail.artist.id),
                            starred: detail.artist.starred,
                            rating: detail.artist.userRating,
                            labeled: true
                        )
                        .buttonStyle(.bordered)
                    }
                    .padding(.horizontal, 18)
                    AlbumCollectionView(
                        albums: detail.albums,
                        highlightedAlbumID: nil,
                        style: .grid,
                        client: client,
                        isAdmin: isAdmin,
                        hasMoreAlbums: false,
                        isLoadingNextPage: false,
                        nextPageError: nil,
                        onHighlight: { _ in },
                        onOpen: onSelectAlbum,
                        onLoadNextPage: {}
                    )
                }
            } else if model.isLoading {
                ProgressView("正在加载艺人…")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if let error = model.errorMessage {
                ContentUnavailableView {
                    Label("无法加载艺人", systemImage: "wifi.exclamationmark")
                } description: {
                    Text(error)
                } actions: {
                    Button("重试") { model.load(artistID: artistID) }
                }
            }
        }
        .task(id: artistID) { model.load(artistID: artistID) }
        .navigationTitle("返回曲库，继续播放")
        .toolbar {
            Button("返回曲库，继续播放", action: onReturn)
        }
    }
}
