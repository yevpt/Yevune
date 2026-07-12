import YevuneCoreFFI
import SwiftUI

struct BatchTagEditorView: View {
    let album: Album
    let trackIDs: [String]
    @ObservedObject var model: MediaViewModel
    let onSuccess: () -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var title = ""
    @State private var albumName = ""
    @State private var artist = ""
    @State private var genre = ""
    @State private var year = ""
    @State private var track = ""
    @State private var discNumber = ""

    init(album: Album, trackIDs: [String], model: MediaViewModel, onSuccess: @escaping () -> Void) {
        self.album = album
        self.trackIDs = trackIDs
        self.model = model
        self.onSuccess = onSuccess
    }

    var body: some View {
        Form {
            Text("仅填写要应用到所选 \(trackIDs.count) 首曲目的共同字段。")
                .foregroundStyle(.secondary)
            TextField("标题", text: $title)
            TextField("专辑", text: $albumName)
            TextField("艺人", text: $artist)
            TextField("流派", text: $genre)
            TextField("年份", text: $year)
            TextField("曲序", text: $track)
            TextField("碟序", text: $discNumber)
            if let errorMessage = model.errorMessage { Text(errorMessage).foregroundStyle(.red) }
            Button("应用标签") {
                Task {
                    let failures = await model.updateTags(ids: trackIDs, update: tagUpdate, album: album)
                    if failures == 0 {
                        onSuccess()
                        dismiss()
                    }
                }
            }
        }
        .padding()
    }

    private var tagUpdate: TagUpdate {
        TagUpdate(
            title: value(title), album: value(albumName), artist: value(artist), genre: value(genre),
            year: UInt32(year), track: UInt32(track), discNumber: UInt32(discNumber)
        )
    }

    private func value(_ text: String) -> String? {
        text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? nil : text
    }
}
