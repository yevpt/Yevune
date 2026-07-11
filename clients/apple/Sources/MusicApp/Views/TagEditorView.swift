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
            Section("整理") {
                TextField("新对象键（library/...）", text: $model.moveKey)
                Button("移动曲目") { Task { await model.move() } }
                Button("删除曲目", role: .destructive) { Task { await model.delete() } }
            }
        }
        .padding()
    }
}
