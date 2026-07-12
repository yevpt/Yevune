import CoreFFI
import Foundation

enum ImportTaskState: Equatable { case waiting, uploading, succeeded, failed }

struct ImportTask: Identifiable {
    let id: UUID
    let url: URL
    var progress: Double
    var state: ImportTaskState
    var track: Track?
    var errorMessage: String?
}

@MainActor
final class LibraryWorkflowViewModel: ObservableObject {
    @Published private(set) var imports: [ImportTask] = []
    @Published private(set) var scanResult: DetailedScanResult?
    @Published private(set) var scanError: String?
    @Published private(set) var isScanning = false
    @Published var isDrawerPresented = false
    @Published private(set) var newAlbumIDs: Set<String> = []

    private let client: any MusicClientProviding
    private let library: LibraryViewModel

    init(client: any MusicClientProviding, library: LibraryViewModel) {
        self.client = client
        self.library = library
    }

    func importFiles(_ urls: [URL]) async {
        guard !urls.isEmpty else { return }
        isDrawerPresented = true
        let ids = urls.map { url -> UUID in
            let id = UUID()
            imports.append(ImportTask(id: id, url: url, progress: 0, state: .waiting))
            return id
        }
        for (id, url) in zip(ids, urls) {
            update(id) { $0.state = .uploading }
            let progress = WorkbenchProgressSink { [weak self] sent, total in
                Task { @MainActor in self?.update(id) { $0.progress = total == 0 ? 0 : Double(sent) / Double(total) } }
            }
            do {
                let track = try await client.upload(localPath: url.path, libraryKey: "library/\(url.lastPathComponent)", progress: progress)
                update(id) { $0.state = .succeeded; $0.progress = 1; $0.track = track }
            } catch {
                update(id) { $0.state = .failed; $0.errorMessage = error.localizedDescription }
            }
        }
        if imports.contains(where: { ids.contains($0.id) && $0.state == .succeeded }) {
            await scanLibrary()
        }
    }

    func scanLibrary() async {
        isDrawerPresented = true
        isScanning = true
        scanError = nil
        defer { isScanning = false }
        do {
            let result = try await client.scanPrefix("library/")
            scanResult = result
            let uploadedAlbums = imports.compactMap { $0.state == .succeeded ? $0.track?.albumId : nil }
            let scannedAlbums = result.changes.compactMap { $0.action == .added ? $0.track.albumId : nil }
            newAlbumIDs = Set(uploadedAlbums + scannedAlbums)
            await library.load()
        } catch { scanError = error.localizedDescription }
    }

    private func update(_ id: UUID, change: (inout ImportTask) -> Void) {
        guard let index = imports.firstIndex(where: { $0.id == id }) else { return }
        change(&imports[index])
    }
}

private final class WorkbenchProgressSink: UploadProgress, @unchecked Sendable {
    private let update: @Sendable (UInt64, UInt64) -> Void
    init(update: @escaping @Sendable (UInt64, UInt64) -> Void) { self.update = update }
    func onProgress(sentBytes: UInt64, totalBytes: UInt64) { update(sentBytes, totalBytes) }
}
