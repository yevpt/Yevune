import SwiftUI
import YevuneCoreFFI

struct AlbumHeaderView: View {
    let album: Album
    let detail: AlbumDetail?
    let coverURL: URL?
    let coverRevision: Int
    let availableWidth: CGFloat
    let isAdmin: Bool
    let onPlay: () -> Void
    let onReplaceCover: () -> Void
    let onManageAlbumAccess: () -> Void
    let onManageArtistAccess: () -> Void
    let onEditAlbum: () -> Void

    private var isWide: Bool { availableWidth >= 620 }
    private var artworkSize: CGFloat { isWide ? 200 : 144 }
    private var gridMetrics: AlbumWorkbenchGridMetrics {
        AlbumWorkbenchPolicy.gridMetrics(width: availableWidth)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(alignment: .top, spacing: isWide ? 24 : 16) {
                AuthenticatedArtworkView(url: coverURL) {
                    Rectangle().fill(.quaternary)
                }
                .id(coverRevision)
                .frame(width: artworkSize, height: artworkSize)
                .clipShape(RoundedRectangle(cornerRadius: 8))
                .accessibilityLabel("\(album.name) 的封面")

                VStack(alignment: .leading, spacing: 12) {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(album.name)
                            .font(isWide ? .largeTitle : .title)
                            .fontWeight(.semibold)
                            .lineLimit(2)
                        Text(album.artist ?? "未知艺人")
                            .font(.title3)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }

                    HStack(spacing: 8) {
                        Button(action: onPlay) {
                            Label("播放专辑", systemImage: "play.fill")
                        }
                        .buttonStyle(.borderedProminent)

                        if isAdmin {
                            Button(action: onReplaceCover) {
                                Label("替换封面", systemImage: "photo")
                            }
                            .buttonStyle(.bordered)

                            Menu {
                                Button("编辑专辑信息…", action: onEditAlbum)
                                Divider()
                                Button("专辑可见范围…", action: onManageAlbumAccess)
                                if album.artistId != nil {
                                    Button("艺人可见范围…", action: onManageArtistAccess)
                                }
                            } label: {
                                Label("管理专辑", systemImage: "ellipsis.circle")
                            }
                        }
                    }
                }
                .frame(maxWidth: .infinity, minHeight: artworkSize, alignment: .topLeading)
            }

            VStack(alignment: .leading, spacing: 2) {
                Text("唱片标签")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Text(AlbumWorkbenchPolicy.metadata(album: album, tracks: detail?.tracks ?? []))
                    .font(.callout.monospacedDigit())
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .accessibilityLabel("唱片标签，\(AlbumWorkbenchPolicy.metadata(album: album, tracks: detail?.tracks ?? []))")
            }
            .padding(.leading, gridMetrics.titleLeadingOffset)
        }
    }
}
