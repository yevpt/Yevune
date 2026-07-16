import YevuneCoreFFI
import Foundation

@MainActor
final class TagEditorViewModel: ObservableObject {
    @Published var title = ""
    @Published var album = ""
    @Published var artist = ""
    @Published var genre = ""
    @Published var year = ""
    @Published var track = ""
    @Published var discNumber = ""
    @Published var moveKey = ""
    @Published private(set) var didSave = false
    @Published private(set) var didDelete = false
    @Published private(set) var didMove = false
    @Published private(set) var errorMessage: String?

    private let client: any MusicClientProviding
    private let trackID: String

    init(client: any MusicClientProviding, trackID: String) {
        self.client = client
        self.trackID = trackID
    }

    init(client: any MusicClientProviding, track: Track) {
        self.client = client
        trackID = track.id
        title = track.title
        album = track.album ?? ""
        artist = track.artist ?? ""
        genre = track.genre ?? ""
        year = track.year.map(String.init) ?? ""
        self.track = track.track.map(String.init) ?? ""
        discNumber = track.discNumber.map(String.init) ?? ""
        moveKey = track.path ?? ""
    }

    func save() async {
        didSave = false
        errorMessage = nil
        do {
            try await client.updateTags(id: trackID, update: TagUpdate(
                title: value(title), album: value(album), artist: value(artist), genre: value(genre),
                year: UInt32(year), track: UInt32(track), discNumber: UInt32(discNumber), clearFields: []
            ))
            didSave = true
        } catch {
            errorMessage = LibraryOperationErrorPresentation.message(error)
        }
    }

    func delete() async {
        didDelete = false
        errorMessage = nil
        do {
            try await client.deleteTrack(id: trackID)
            didDelete = true
        }
        catch { errorMessage = LibraryOperationErrorPresentation.message(error) }
    }

    func move() async {
        didMove = false
        errorMessage = nil
        do {
            try await client.moveTrack(id: trackID, key: moveKey)
            didMove = true
        }
        catch { errorMessage = LibraryOperationErrorPresentation.message(error) }
    }

    private func value(_ text: String) -> String? {
        text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? nil : text
    }
}
