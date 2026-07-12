import YevuneCoreFFI
import SwiftUI

func coverArtID(for album: Album) -> String? {
    album.coverArt
}

func loadCoverURL(for album: Album, client: any MusicClientProviding) async -> URL? {
    guard let coverArtID = coverArtID(for: album),
          let urlString = try? await client.coverArtURL(id: coverArtID, size: 300) else {
        return nil
    }
    return URL(string: urlString)
}

struct AlbumGridView: View {
    let albums: [Album]
    let client: any MusicClientProviding
    let newAlbumIDs: Set<String>
    let onSelect: (Album) -> Void

    private let columns = [GridItem(.adaptive(minimum: 150), spacing: 20)]

    var body: some View {
        ScrollView {
            LazyVGrid(columns: columns, spacing: 24) {
                ForEach(albums, id: \.id) { album in
                    AlbumGridCell(album: album, client: client, isNew: newAlbumIDs.contains(album.id))
                        .onTapGesture { onSelect(album) }
                }
            }
            .padding()
        }
    }
}

private struct AlbumGridCell: View {
    let album: Album
    let client: any MusicClientProviding
    let isNew: Bool
    @State private var coverURL: URL?

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            AsyncImage(url: coverURL) { image in
                image.resizable().scaledToFill()
            } placeholder: {
                Color.secondary.opacity(0.15)
            }
            .frame(width: 150, height: 150)
            .clipped()
            .cornerRadius(8)

            HStack {
                Text(album.name).font(.subheadline.bold()).lineLimit(1)
                if isNew {
                    Text("新增").font(.caption2).padding(.horizontal, 5).background(.green.opacity(0.2), in: Capsule())
                }
            }
            Text(album.artist ?? "未知艺人").font(.caption).foregroundStyle(.secondary).lineLimit(1)
        }
        .frame(width: 150)
        .task(id: album.coverArt) {
            coverURL = await loadCoverURL(for: album, client: client)
        }
    }
}
