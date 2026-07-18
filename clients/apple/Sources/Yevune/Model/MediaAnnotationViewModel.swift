import Foundation
import YevuneCoreFFI

enum MediaAnnotationTarget: Hashable {
    case track(String)
    case album(String)
    case artist(String)

    var id: String {
        switch self {
        case .track(let id), .album(let id), .artist(let id): id
        }
    }

    var itemType: AnnotationItemType {
        switch self {
        case .track: .track
        case .album: .album
        case .artist: .artist
        }
    }
}

struct MediaAnnotationSnapshot: Equatable {
    let isStarred: Bool
    let rating: UInt8?
}

@MainActor
final class MediaAnnotationViewModel: ObservableObject {
    @Published private var snapshots: [MediaAnnotationTarget: MediaAnnotationSnapshot] = [:]
    @Published private var mutatingTargets: Set<MediaAnnotationTarget> = []
    @Published private var errors: [MediaAnnotationTarget: String] = [:]

    private let client: any MediaAnnotationProviding

    init(client: any MediaAnnotationProviding) {
        self.client = client
    }

    func seed(track: Track) {
        seed(.track(track.id), starred: track.starred, rating: track.userRating)
    }

    func seed(album: Album) {
        seed(.album(album.id), starred: album.starred, rating: album.userRating)
    }

    func seed(artist: Artist) {
        seed(.artist(artist.id), starred: artist.starred, rating: artist.userRating)
    }

    func seed(target: MediaAnnotationTarget, starred: String?, rating: UInt8?) {
        seed(target, starred: starred, rating: rating)
    }

    func snapshot(for target: MediaAnnotationTarget) -> MediaAnnotationSnapshot? {
        snapshots[target]
    }

    func snapshot(
        for target: MediaAnnotationTarget,
        fallbackStarred: Bool,
        fallbackRating: UInt8?
    ) -> MediaAnnotationSnapshot {
        snapshots[target] ?? MediaAnnotationSnapshot(
            isStarred: fallbackStarred,
            rating: fallbackRating
        )
    }

    func isMutating(_ target: MediaAnnotationTarget) -> Bool {
        mutatingTargets.contains(target)
    }

    func error(for target: MediaAnnotationTarget) -> String? {
        errors[target]
    }

    func clearError(for target: MediaAnnotationTarget) {
        errors[target] = nil
    }

    @discardableResult
    func setStarred(target: MediaAnnotationTarget, starred: Bool) async -> Bool {
        await mutate(target) {
            try await self.client.setStarred(
                id: target.id,
                itemType: target.itemType,
                starred: starred
            )
        }
    }

    @discardableResult
    func setRating(target: MediaAnnotationTarget, rating: UInt8?) async -> Bool {
        await mutate(target) {
            try await self.client.setRating(id: target.id, rating: rating)
        }
    }

    private func seed(
        _ target: MediaAnnotationTarget,
        starred: String?,
        rating: UInt8?
    ) {
        guard !mutatingTargets.contains(target), snapshots[target] == nil else { return }
        snapshots[target] = MediaAnnotationSnapshot(
            isStarred: starred != nil,
            rating: rating
        )
    }

    private func mutate(
        _ target: MediaAnnotationTarget,
        operation: () async throws -> Void
    ) async -> Bool {
        guard mutatingTargets.insert(target).inserted else { return false }
        errors[target] = nil
        defer { mutatingTargets.remove(target) }
        do {
            try await operation()
            snapshots[target] = try await refreshedSnapshot(for: target)
            return true
        } catch {
            errors[target] = LibraryOperationErrorPresentation.message(error)
            return false
        }
    }

    private func refreshedSnapshot(for target: MediaAnnotationTarget) async throws -> MediaAnnotationSnapshot {
        switch target {
        case .track(let id):
            let track = try await client.getSong(id: id)
            guard track.id == id else { throw CocoaError(.validationMissingMandatoryProperty) }
            return MediaAnnotationSnapshot(isStarred: track.starred != nil, rating: track.userRating)
        case .album(let id):
            let album = try await client.getAlbum(id: id).album
            guard album.id == id else { throw CocoaError(.validationMissingMandatoryProperty) }
            return MediaAnnotationSnapshot(isStarred: album.starred != nil, rating: album.userRating)
        case .artist(let id):
            let artist = try await client.getArtist(id: id).artist
            guard artist.id == id else { throw CocoaError(.validationMissingMandatoryProperty) }
            return MediaAnnotationSnapshot(isStarred: artist.starred != nil, rating: artist.userRating)
        }
    }
}
