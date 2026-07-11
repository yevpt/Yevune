import SwiftUI

struct TagEditorView: View {
    @ObservedObject var model: TagEditorViewModel

    var body: some View {
        Form {
            TextField("标题", text: $model.title)
            TextField("专辑", text: $model.album)
            TextField("艺人", text: $model.artist)
            TextField("流派", text: $model.genre)
            TextField("年份", text: $model.year)
            TextField("曲序", text: $model.track)
            TextField("碟序", text: $model.discNumber)
            if let errorMessage = model.errorMessage { Text(errorMessage).foregroundStyle(.red) }
            Button("保存标签覆盖") { Task { await model.save() } }
        }
        .padding()
    }
}
