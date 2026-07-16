import Foundation
import YevuneCoreFFI

@MainActor
final class ArtistDetailViewModel: ObservableObject {
    @Published private(set) var detail: ArtistDetail?
    @Published private(set) var isLoading = false
    @Published private(set) var errorMessage: String?

    private let client: any MusicClientProviding
    private var task: Task<Void, Never>?
    private var generation = 0

    init(client: any MusicClientProviding) {
        self.client = client
    }

    func load(artistID: String) {
        generation += 1
        let expectedGeneration = generation
        task?.cancel()

        task = Task { [weak self] in
            guard let self else { return }
            guard expectedGeneration == generation else { return }
            isLoading = true
            errorMessage = nil

            do {
                let value = try await client.getArtist(id: artistID)
                guard expectedGeneration == generation else { return }
                detail = value
                isLoading = false
            } catch {
                guard expectedGeneration == generation else { return }
                errorMessage = error.localizedDescription
                isLoading = false
            }
        }
    }
}
