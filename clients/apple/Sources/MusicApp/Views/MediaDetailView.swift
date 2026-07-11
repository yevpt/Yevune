import CoreFFI
import SwiftUI

struct MediaDetailView: View {
    let album: Album
    @ObservedObject var model: MediaViewModel
    @State private var importing = false

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(alignment: .top) {
                AsyncImage(url: model.coverURL) { image in image.resizable().scaledToFill() } placeholder: { Color.secondary.opacity(0.15) }
                    .frame(width: 180, height: 180).clipped().cornerRadius(8)
                VStack(alignment: .leading) {
                    Text(album.name).font(.largeTitle)
                    Text(album.artist ?? "未知艺人").foregroundStyle(.secondary)
                    Button("替换封面") { importing = true }
                }
            }
            if let detail = model.detail {
                List(detail.tracks, id: \.id) { track in
                    HStack { Text(track.title); Spacer(); Button(model.playingTrackID == track.id ? "暂停" : "试听") { Task { await model.toggle(track: track) } } }
                }
            }
            if let error = model.errorMessage { Text(error).foregroundStyle(.red) }
        }.padding().task(id: album.id) { await model.load(album: album) }
        .fileImporter(isPresented: $importing, allowedContentTypes: [.image]) { result in
            if case let .success(url) = result { Task { await model.replaceCover(albumID: album.id, path: url.path) } }
        }
    }
}
