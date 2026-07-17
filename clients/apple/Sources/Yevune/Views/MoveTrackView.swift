import SwiftUI

struct MoveTrackView: View {
    @ObservedObject var model: MoveTrackViewModel
    let onSuccess: (String) -> Void
    @Environment(\.dismiss) private var dismiss

    init(model: MoveTrackViewModel, onSuccess: @escaping (String) -> Void = { _ in }) {
        self.model = model
        self.onSuccess = onSuccess
    }

    var body: some View {
        NavigationStack {
            Form {
                Section("当前曲目") {
                    LabeledContent("标题", value: model.track.title)
                    LabeledContent("当前路径", value: model.track.path ?? "未知")
                }

                Section("目标曲库路径") {
                    TextField("library/…", text: $model.destination)
                    if let pathError = model.pathError {
                        Text(pathError)
                            .font(.caption)
                            .foregroundStyle(.red)
                    }
                    Text("曲目只能移动到 library/ 下；目标冲突与权限由服务器最终校验。")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }

                if let errorMessage = model.errorMessage {
                    Section {
                        Label(errorMessage, systemImage: "exclamationmark.circle")
                            .foregroundStyle(.red)
                    }
                }
            }
            .formStyle(.grouped)
            .navigationTitle("移动曲目")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("取消") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("移动") { Task { await model.submit() } }
                        .disabled(!model.canSubmit)
                }
            }
        }
        .frame(minWidth: 520, minHeight: 330)
        .onChange(of: model.didMove) { _, didMove in
            guard didMove else { return }
            onSuccess("曲目已移动")
            dismiss()
        }
    }
}
