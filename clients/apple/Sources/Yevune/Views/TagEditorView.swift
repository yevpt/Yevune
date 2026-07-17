import SwiftUI

struct TagEditorView: View {
    @ObservedObject var model: TagEditorViewModel
    let onSuccess: (String) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var confirmingDiscard = false

    init(model: TagEditorViewModel, onSuccess: @escaping (String) -> Void = { _ in }) {
        self.model = model
        self.onSuccess = onSuccess
    }

    var body: some View {
        NavigationStack {
            Form {
                Section("歌曲信息") {
                    field("标题", text: $model.draft.title, error: model.validation.title)
                    field("专辑", text: $model.draft.album)
                    field("艺人", text: $model.draft.artist)
                    field("流派", text: $model.draft.genre)
                    field("年份", text: $model.draft.year, error: model.validation.year)
                }

                Section("排序信息") {
                    field("曲序", text: $model.draft.track, error: model.validation.track)
                    field("碟序", text: $model.draft.discNumber, error: model.validation.discNumber)
                }

                if let errorMessage = model.errorMessage {
                    Section {
                        Label(errorMessage, systemImage: "exclamationmark.circle")
                            .foregroundStyle(.red)
                    }
                }
            }
            .formStyle(.grouped)
            .navigationTitle("修改标签")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("取消") { requestDismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("保存更改") { Task { await model.save() } }
                        .disabled(!model.canSave)
                }
            }
        }
        .frame(minWidth: 480, minHeight: 460)
        .confirmationDialog(
            "放弃未保存的更改？",
            isPresented: $confirmingDiscard,
            titleVisibility: .visible
        ) {
            Button("放弃更改", role: .destructive) { dismiss() }
            Button("继续编辑", role: .cancel) {}
        }
        .onChange(of: model.didSave) { _, didSave in
            guard didSave else { return }
            onSuccess("标签已更新")
            dismiss()
        }
    }

    @ViewBuilder
    private func field(_ title: String, text: Binding<String>, error: String? = nil) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            TextField(title, text: text)
            if let error {
                Text(error)
                    .font(.caption)
                    .foregroundStyle(.red)
                    .accessibilityLabel("\(title)错误：\(error)")
            }
        }
    }

    private func requestDismiss() {
        if model.isDirty {
            confirmingDiscard = true
        } else {
            dismiss()
        }
    }
}
