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
    private var gridMetrics: AlbumWorkbenchGridMetrics {
        AlbumWorkbenchPolicy.gridMetrics(width: availableWidth)
    }
    private var metadata: String {
        AlbumWorkbenchPolicy.metadata(album: album, tracks: detail?.tracks ?? [])
    }

    var body: some View {
        HStack(alignment: .top, spacing: gridMetrics.headerSpacing) {
            AuthenticatedArtworkView(url: coverURL) {
                Rectangle().fill(.quaternary)
            }
            .id(coverRevision)
            .frame(width: gridMetrics.artworkSize, height: gridMetrics.artworkSize)
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

                Spacer(minLength: 0)

                HStack(spacing: 8) {
                    Text("唱片标签")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    Text(metadata)
                        .font(.callout.monospacedDigit())
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
                .accessibilityElement(children: .ignore)
                .accessibilityLabel("唱片标签，\(metadata)")
            }
            .frame(maxWidth: .infinity, minHeight: gridMetrics.artworkSize, alignment: .topLeading)
        }
        .padding(.horizontal, gridMetrics.outerHorizontalInset)
    }
}
