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
        moveKey = track.path ?? ""
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

    // Temporary bindings retained only so the pre-Task-8 view keeps compiling.
    var title: String {
        get { draft.title }
        set { draft.title = newValue }
    }

    var album: String {
        get { draft.album }
        set { draft.album = newValue }
    }

    var artist: String {
        get { draft.artist }
        set { draft.artist = newValue }
    }

    var genre: String {
        get { draft.genre }
        set { draft.genre = newValue }
    }

    var year: String {
        get { draft.year }
        set { draft.year = newValue }
    }

    var track: String {
        get { draft.track }
        set { draft.track = newValue }
    }

    var discNumber: String {
        get { draft.discNumber }
        set { draft.discNumber = newValue }
    }

    // Move/delete remain a minimal compatibility shim until Task 8 removes their UI.
    @Published var moveKey = ""
    @Published private(set) var didDelete = false
    @Published private(set) var didMove = false

    func delete() async {
        didDelete = false
        errorMessage = nil
        do {
            try await client.deleteTrack(id: trackID)
            didDelete = true
        } catch {
            errorMessage = LibraryOperationErrorPresentation.message(error)
        }
    }

    func move() async {
        didMove = false
        errorMessage = nil
        do {
            try await client.moveTrack(id: trackID, key: moveKey)
            didMove = true
        } catch {
            errorMessage = LibraryOperationErrorPresentation.message(error)
        }
    }
}
