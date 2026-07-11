import SwiftUI
import UniformTypeIdentifiers

struct UploadView: View {
    @ObservedObject var model: UploadViewModel
    @State private var isTargeted = false

    var body: some View {
        VStack(spacing: 16) {
            Text("拖入音频文件上传")
                .font(.title2.weight(.semibold))
            Text("文件会直接流式传至家庭曲库")
                .foregroundStyle(.secondary)
            ProgressView(value: model.progress)
                .opacity(model.isUploading ? 1 : 0)
            if let errorMessage = model.errorMessage {
                Text(errorMessage).foregroundStyle(.red)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding()
        .background(isTargeted ? .orange.opacity(0.16) : .indigo.opacity(0.08))
        .onDrop(of: [.fileURL], isTargeted: $isTargeted) { providers in
            guard let provider = providers.first else { return false }
            provider.loadItem(forTypeIdentifier: UTType.fileURL.identifier, options: nil) { item, _ in
                guard let data = item as? Data,
                      let url = URL(dataRepresentation: data, relativeTo: nil) else { return }
                let key = "library/\(url.lastPathComponent)"
                Task { await model.upload(localPath: url.path, libraryKey: key) }
            }
            return true
        }
    }
}
