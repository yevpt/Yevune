import Foundation
import YevuneCoreFFI

@MainActor
final class TagEditorViewModel: ObservableObject {
    @Published var draft: TagDraft
    @Published private(set) var didSave = false
    @Published private(set) var isSubmitting = false
    @Published private(set) var errorMessage: String?

    private let client: any MusicClientProviding
    private let trackID: String

    init(client: any MusicClientProviding, trackID: String) {
        self.client = client
        self.trackID = trackID
        draft = TagDraft()
    }

    init(client: any MusicClientProviding, track: Track) {
        self.client = client
        trackID = track.id
        draft = TagDraft(track: track)
    }

    var validation: TagDraftValidation { draft.validation }
    var isDirty: Bool { draft.isDirty }
    var canSave: Bool { isDirty && validation.isValid && !isSubmitting }

    func save() async {
        guard canSave, let update = draft.makeUpdate() else { return }
        isSubmitting = true
        didSave = false
        errorMessage = nil
        defer { isSubmitting = false }
        do {
            try await client.updateTags(id: trackID, update: update)
            didSave = true
        } catch {
            errorMessage = LibraryOperationErrorPresentation.message(error)
        }
    }

}
