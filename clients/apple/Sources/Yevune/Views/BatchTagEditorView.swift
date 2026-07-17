import SwiftUI
import YevuneCoreFFI

struct BatchTagEditorView: View {
    let trackCount: Int
    let scopeExplanation: String?
    let onSubmit: (TagUpdate) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var draft = BatchTagDraft()

    init(
        trackCount: Int,
        scopeExplanation: String? = nil,
        onSubmit: @escaping (TagUpdate) -> Void
    ) {
        self.trackCount = trackCount
        self.scopeExplanation = scopeExplanation
        self.onSubmit = onSubmit
    }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    Text(scopeExplanation ?? "只会更改所选 \(trackCount) 首曲目的公共字段。")
                        .foregroundStyle(.secondary)
                }

                batchField("专辑", keyPath: \.album)
                batchField("艺人", keyPath: \.artist)
                batchField("流派", keyPath: \.genre)
                batchField("年份", keyPath: \.year, error: draft.validation.year)
            }
            .formStyle(.grouped)
            .navigationTitle("批量修改标签")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("取消") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("应用更改") {
                        guard let update = draft.makeUpdate() else { return }
                        onSubmit(update)
                        dismiss()
                    }
                    .disabled(draft.makeUpdate() == nil)
                }
            }
        }
        .frame(minWidth: 500, minHeight: 430)
    }

    @ViewBuilder
    private func batchField(
        _ title: String,
        keyPath: WritableKeyPath<BatchTagDraft, BatchFieldMode>,
        error: String? = nil
    ) -> some View {
        Section(title) {
            Picker(title, selection: choiceBinding(keyPath)) {
                Text("保持").tag(BatchChoice.keep)
                Text("设置").tag(BatchChoice.set)
                Text("清空").tag(BatchChoice.clear)
            }
            .pickerStyle(.segmented)
            .labelsHidden()

            if choice(for: draft[keyPath: keyPath]) == .set {
                TextField(title, text: valueBinding(keyPath))
                if let message = error ?? blankError(for: draft[keyPath: keyPath], title: title) {
                    Text(message)
                        .font(.caption)
                        .foregroundStyle(.red)
                }
            }
        }
    }

    private func choiceBinding(
        _ keyPath: WritableKeyPath<BatchTagDraft, BatchFieldMode>
    ) -> Binding<BatchChoice> {
        Binding(
            get: { choice(for: draft[keyPath: keyPath]) },
            set: { newChoice in
                switch newChoice {
                case .keep: draft[keyPath: keyPath] = .keep
                case .set: draft[keyPath: keyPath] = .set("")
                case .clear: draft[keyPath: keyPath] = .clear
                }
            }
        )
    }

    private func valueBinding(
        _ keyPath: WritableKeyPath<BatchTagDraft, BatchFieldMode>
    ) -> Binding<String> {
        Binding(
            get: {
                guard case let .set(value) = draft[keyPath: keyPath] else { return "" }
                return value
            },
            set: { draft[keyPath: keyPath] = .set($0) }
        )
    }

    private func choice(for mode: BatchFieldMode) -> BatchChoice {
        switch mode {
        case .keep: .keep
        case .set: .set
        case .clear: .clear
        }
    }

    private func blankError(for mode: BatchFieldMode, title: String) -> String? {
        guard case let .set(value) = mode,
              value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return nil }
        return "\(title)不能为空"
    }
}

private enum BatchChoice: Hashable {
    case keep
    case set
    case clear
}
