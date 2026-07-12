import YevuneCoreFFI
import Foundation

@MainActor
final class UploadViewModel: ObservableObject {
    @Published private(set) var progress = 0.0
    @Published private(set) var errorMessage: String?
    @Published private(set) var isUploading = false

    private let client: any MusicClientProviding

    init(client: any MusicClientProviding) {
        self.client = client
    }

    func upload(localPath: String, libraryKey: String) async {
        isUploading = true
        progress = 0
        errorMessage = nil
        defer { isUploading = false }
        let sink = UploadProgressSink { [weak self] sent, total in
            Task { @MainActor in
                self?.progress = total == 0 ? 0 : Double(sent) / Double(total)
            }
        }
        do {
            _ = try await client.upload(localPath: localPath, libraryKey: libraryKey, progress: sink)
        } catch {
            errorMessage = error.localizedDescription
        }
    }
}

private final class UploadProgressSink: UploadProgress, @unchecked Sendable {
    private let update: @Sendable (UInt64, UInt64) -> Void

    init(update: @escaping @Sendable (UInt64, UInt64) -> Void) {
        self.update = update
    }

    func onProgress(sentBytes: UInt64, totalBytes: UInt64) {
        update(sentBytes, totalBytes)
    }
}
