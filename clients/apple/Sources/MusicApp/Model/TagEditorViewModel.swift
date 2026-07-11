import CoreFFI
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
    @Published private(set) var errorMessage: String?

    private let client: any MusicClientProviding
    private let trackID: String

    init(client: any MusicClientProviding, trackID: String) {
        self.client = client
        self.trackID = trackID
    }

    func save() async {
        didSave = false
        errorMessage = nil
        do {
            try await client.updateTags(id: trackID, update: TagUpdate(
                title: value(title), album: value(album), artist: value(artist), genre: value(genre),
                year: UInt32(year), track: UInt32(track), discNumber: UInt32(discNumber)
            ))
            didSave = true
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func delete() async {
        errorMessage = nil
        do { try await client.deleteTrack(id: trackID) }
        catch { errorMessage = error.localizedDescription }
    }

    func move() async {
        errorMessage = nil
        do { try await client.moveTrack(id: trackID, key: moveKey) }
        catch { errorMessage = error.localizedDescription }
    }

    private func value(_ text: String) -> String? {
        text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? nil : text
    }
}
