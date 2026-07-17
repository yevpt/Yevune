import Foundation
import YevuneCoreFFI

@MainActor
final class MoveTrackViewModel: ObservableObject {
    @Published var destination: String
    @Published private(set) var isSubmitting = false
    @Published private(set) var didMove = false
    @Published private(set) var errorMessage: String?

    let track: Track
    private let client: any MusicClientProviding
    private let originalPath: String

    init(client: any MusicClientProviding, track: Track) {
        self.client = client
        self.track = track
        originalPath = track.path ?? ""
        destination = track.path ?? ""
    }

    var pathError: String? {
        let value = destination.trimmingCharacters(in: .whitespacesAndNewlines)
        if !value.hasPrefix("library/") { return "路径必须以 library/ 开头" }
        if value.contains("..") { return "路径不能包含 .." }
        if value == originalPath { return "请输入不同的目标路径" }
        return nil
    }

    var canSubmit: Bool { pathError == nil && !isSubmitting }

    func submit() async {
        guard canSubmit else { return }
        let value = destination.trimmingCharacters(in: .whitespacesAndNewlines)
        isSubmitting = true
        didMove = false
        errorMessage = nil
        defer { isSubmitting = false }

        do {
            try await client.moveTrack(id: track.id, key: value)
            didMove = true
        } catch {
            errorMessage = LibraryOperationErrorPresentation.message(error)
        }
    }
}
