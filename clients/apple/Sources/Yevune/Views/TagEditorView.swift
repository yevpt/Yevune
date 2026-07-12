import SwiftUI

struct TagEditorView: View {
    @ObservedObject var model: TagEditorViewModel
    let onSuccess: (String) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var confirmingDelete = false

    init(model: TagEditorViewModel, onSuccess: @escaping (String) -> Void = { _ in }) {
        self.model = model
        self.onSuccess = onSuccess
    }

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
            Button("保存标签") { Task { await model.save() } }
            Section("整理") {
                TextField("新对象键（library/...）", text: $model.moveKey)
                Button("移动曲目") { Task { await model.move() } }
                    .disabled(model.moveKey.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                Button("删除曲目", role: .destructive) { confirmingDelete = true }
            }
        }
        .padding()
        .confirmationDialog("确定删除这首曲目吗？", isPresented: $confirmingDelete, titleVisibility: .visible) {
            Button("删除", role: .destructive) { Task { await model.delete() } }
        } message: {
            Text("此操作无法撤销。")
        }
        .onChange(of: model.didSave) { _, didSave in
            if didSave { complete("标签已保存") }
        }
        .onChange(of: model.didMove) { _, didMove in
            if didMove { complete("曲目已移动") }
        }
        .onChange(of: model.didDelete) { _, didDelete in
            if didDelete { complete("曲目已删除") }
        }
    }

    private func complete(_ message: String) {
        onSuccess(message)
        dismiss()
    }
}
