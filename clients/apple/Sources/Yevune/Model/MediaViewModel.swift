import Foundation
import YevuneCoreFFI

enum AlbumDetailPhase: Equatable {
    case idle
    case loading
    case content
    case refreshing
    case failed(String)
}

@MainActor
final class MediaViewModel: ObservableObject {
    @Published private(set) var detail: AlbumDetail?
    @Published private(set) var coverURL: URL?
    @Published private(set) var errorMessage: String?
    @Published private(set) var operationMessage: String?
    @Published private(set) var currentAlbumID: String?
    @Published private(set) var coverRevision = 0
    @Published private(set) var phase: AlbumDetailPhase = .idle
    @Published private(set) var refreshError: String?
    @Published private(set) var coverError: String?
    @Published private(set) var operationError: String?

    private let client: any MusicClientProviding
    private var generation = 0
    private var detailTask: Task<AlbumDetail, Error>?
    private var coverTask: Task<URL?, Error>?
    private var coverURLCoverID: String?

    init(client: any MusicClientProviding) {
        self.client = client
    }

    func load(album: Album) async {
        _ = await performLoad(album: album)
    }

    func makeTagEditor(for track: Track) -> TagEditorViewModel {
        TagEditorViewModel(client: client, track: track)
    }

    func refresh(album: Album, successMessage: String) async {
        if currentAlbumID == album.id {
            operationMessage = nil
        }
        let requestGeneration = await performLoad(album: album)
        guard isCurrent(requestGeneration, albumID: album.id) else { return }
        guard refreshError == nil, coverError == nil, phase == .content else { return }
        operationMessage = successMessage
    }

    @discardableResult
    func updateTags(ids: [String], update: TagUpdate, album: Album) async -> Int {
        operationError = nil
        operationMessage = nil
        var failures = 0
        for id in ids {
            do {
                try await client.updateTags(id: id, update: update)
            } catch {
                failures += 1
            }
        }
        await load(album: album)
        if failures > 0 {
            operationError = "\(failures) 项操作失败"
        } else {
            operationMessage = "已更新 \(ids.count) 首曲目的标签"
        }
        synchronizeLegacyError()
        return failures
    }

    @discardableResult
    func deleteTracks(ids: [String], album: Album) async -> Int {
        operationError = nil
        operationMessage = nil
        var failures = 0
        for id in ids {
            do {
                try await client.deleteTrack(id: id)
            } catch {
                failures += 1
            }
        }
        await load(album: album)
        if failures > 0 {
            operationError = "\(failures) 项操作失败"
        } else {
            operationMessage = "已删除 \(ids.count) 首曲目"
        }
        synchronizeLegacyError()
        return failures
    }

    func replaceCover(album: Album, path: String) async {
        let replacementGeneration = generation
        guard currentAlbumID == album.id else { return }
        operationError = nil
        operationMessage = nil
        synchronizeLegacyError()

        do {
            try await client.setCoverArt(albumID: album.id, localPath: path)
            guard isCurrent(replacementGeneration, albumID: album.id) else { return }

            let refreshGeneration = await performLoad(album: album)
            guard isCurrent(refreshGeneration, albumID: album.id) else { return }
            guard refreshError == nil,
                  coverError == nil,
                  phase == .content,
                  detail?.album.coverArt != nil,
                  coverURL != nil else { return }

            coverRevision += 1
            operationMessage = "封面已更新"
        } catch {
            guard isCurrent(replacementGeneration, albumID: album.id) else { return }
            operationError = LibraryOperationErrorPresentation.message(error)
            synchronizeLegacyError()
        }
    }

    // Compatibility for the existing detail view until it migrates to the album-valued API.
    func replaceCover(albumID: String, path: String) async {
        guard currentAlbumID == albumID, let album = detail?.album else { return }
        await replaceCover(album: album, path: path)
    }

    private func performLoad(album: Album) async -> Int {
        generation += 1
        let requestGeneration = generation
        detailTask?.cancel()
        coverTask?.cancel()

        let retainedContent = currentAlbumID == album.id && detail != nil
        if currentAlbumID != album.id {
            detail = nil
            coverURL = nil
            coverURLCoverID = nil
            operationError = nil
            operationMessage = nil
        }
        currentAlbumID = album.id
        refreshError = nil
        coverError = nil
        phase = retainedContent ? .refreshing : .loading
        synchronizeLegacyError()

        let routedCoverID = album.coverArt
        let newDetailTask = Task { [client] in
            try await client.getAlbum(id: album.id)
        }
        let routedCoverTask = makeCoverTask(coverID: routedCoverID)
        detailTask = newDetailTask
        coverTask = routedCoverTask

        let fetchedDetail: AlbumDetail
        do {
            fetchedDetail = try await newDetailTask.value
        } catch {
            guard isCurrent(requestGeneration, albumID: album.id) else { return requestGeneration }
            coverTask?.cancel()
            coverTask = nil
            detailTask = nil
            let message = LibraryOperationErrorPresentation.message(error)
            if retainedContent {
                refreshError = message
                phase = .content
            } else {
                phase = .failed(message)
            }
            synchronizeLegacyError()
            return requestGeneration
        }

        guard isCurrent(requestGeneration, albumID: album.id) else { return requestGeneration }
        detailTask = nil
        detail = fetchedDetail
        phase = .content

        let fetchedCoverID = fetchedDetail.album.coverArt
        if coverURLCoverID != fetchedCoverID {
            coverURL = nil
            coverURLCoverID = nil
        }
        guard let fetchedCoverID else {
            routedCoverTask.cancel()
            coverTask = nil
            coverURL = nil
            coverError = nil
            synchronizeLegacyError()
            return requestGeneration
        }

        let finalCoverTask: Task<URL?, Error>
        if fetchedCoverID == routedCoverID {
            finalCoverTask = routedCoverTask
        } else {
            routedCoverTask.cancel()
            finalCoverTask = makeCoverTask(coverID: fetchedCoverID)
            coverTask = finalCoverTask
        }

        do {
            let resolvedURL = try await finalCoverTask.value
            guard isCurrent(requestGeneration, albumID: album.id),
                  detail?.album.coverArt == fetchedCoverID else { return requestGeneration }
            coverTask = nil
            coverURL = resolvedURL
            coverURLCoverID = fetchedCoverID
            coverError = nil
            synchronizeLegacyError()
        } catch {
            guard isCurrent(requestGeneration, albumID: album.id),
                  detail?.album.coverArt == fetchedCoverID else { return requestGeneration }
            coverTask = nil
            coverURL = nil
            coverURLCoverID = nil
            coverError = LibraryOperationErrorPresentation.message(error)
            synchronizeLegacyError()
        }
        return requestGeneration
    }

    private func makeCoverTask(coverID: String?) -> Task<URL?, Error> {
        Task { [client] in
            guard let coverID else { return nil }
            let value = try await client.coverArtURL(id: coverID, size: 600)
            guard let url = URL(string: value) else { throw URLError(.badURL) }
            return url
        }
    }

    private func isCurrent(_ requestGeneration: Int, albumID: String) -> Bool {
        requestGeneration == generation && albumID == currentAlbumID
    }

    private func synchronizeLegacyError() {
        if case let .failed(message) = phase {
            errorMessage = operationError ?? refreshError ?? coverError ?? message
        } else {
            errorMessage = operationError ?? refreshError ?? coverError
        }
    }
}
