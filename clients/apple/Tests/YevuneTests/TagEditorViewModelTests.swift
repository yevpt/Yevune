import YevuneCoreFFI
import XCTest
@testable import Yevune

@MainActor
final class TagEditorViewModelTests: XCTestCase {
    func testUnchangedSingleDraftProducesNoUpdate() {
        XCTAssertNil(TagDraft(track: trackFixture()).makeUpdate())
    }

    func testUneditedTitleWithOriginalOuterWhitespaceStaysUnchanged() {
        var track = trackFixture()
        track.title = "  Song  "
        let draft = TagDraft(track: track)

        XCTAssertFalse(draft.isDirty)
        XCTAssertNil(draft.makeUpdate())
    }

    func testUneditedOptionalTextWithOriginalOuterWhitespaceStaysUnchanged() {
        var track = trackFixture()
        track.album = "  Album  "
        track.artist = "  Artist  "
        track.genre = "  Rock  "
        let draft = TagDraft(track: track)

        XCTAssertFalse(draft.isDirty)
        XCTAssertNil(draft.makeUpdate())
    }

    func testUneditedWhitespaceOnlyOptionalRetainsNonNilIdentityWithoutClearing() {
        var track = trackFixture()
        track.album = "   "

        XCTAssertNil(TagDraft(track: track).makeUpdate())
    }

    func testChangedSingleDraftTrimsAndProducesOnlyChangedValues() throws {
        var draft = TagDraft(track: trackFixture())
        draft.title = "  Retitled  "
        draft.track = "4"

        let update = try XCTUnwrap(draft.makeUpdate())

        XCTAssertEqual(update.title, "Retitled")
        XCTAssertEqual(update.track, 4)
        XCTAssertNil(update.album)
        XCTAssertNil(update.discNumber)
        XCTAssertEqual(update.clearFields, [])
    }

    func testClearingOptionalFieldsProducesStableClearFields() throws {
        var draft = TagDraft(track: trackFixture())
        draft.album = ""
        draft.artist = ""
        draft.genre = ""
        draft.year = ""
        draft.track = ""
        draft.discNumber = ""

        let update = try XCTUnwrap(draft.makeUpdate())

        XCTAssertEqual(update.clearFields, [.album, .artist, .genre, .year, .track, .discNumber])
        XCTAssertNil(update.album)
        XCTAssertNil(update.artist)
        XCTAssertNil(update.genre)
        XCTAssertNil(update.year)
        XCTAssertNil(update.track)
        XCTAssertNil(update.discNumber)
    }

    func testOriginallyNilOptionalFieldsLeftBlankStayUnchanged() {
        var track = trackFixture()
        track.album = nil
        track.artist = nil
        track.genre = nil
        track.year = nil
        track.track = nil
        track.discNumber = nil

        XCTAssertNil(TagDraft(track: track).makeUpdate())
    }

    func testBlankTitleAndMalformedNumbersBlockSubmission() {
        var draft = TagDraft(track: trackFixture())
        draft.title = "  "
        draft.year = "10000"
        draft.track = "abc"
        draft.discNumber = "0"

        XCTAssertNotNil(draft.validation.title)
        XCTAssertNotNil(draft.validation.year)
        XCTAssertNotNil(draft.validation.track)
        XCTAssertNotNil(draft.validation.discNumber)
        XCTAssertNil(draft.makeUpdate())
    }

    func testMalformedNumericEditIsDirtyButCannotSave() {
        var draft = TagDraft(track: trackFixture())
        draft.track = "abc"

        XCTAssertTrue(draft.isDirty)
        XCTAssertFalse(draft.validation.isValid)
    }

    func testNumericBoundsAreAccepted() throws {
        var draft = TagDraft(track: trackFixture())
        draft.year = "1"
        draft.track = "999"
        draft.discNumber = "1"

        let update = try XCTUnwrap(draft.makeUpdate())

        XCTAssertEqual(update.year, 1)
        XCTAssertEqual(update.track, 999)
        XCTAssertEqual(update.discNumber, 1)
    }

    func testBatchDraftOnlyBuildsSafeCommonFields() throws {
        var draft = BatchTagDraft()
        draft.album = .set("  Collection  ")
        draft.artist = .keep
        draft.genre = .clear
        draft.year = .set("2025")

        let update = try XCTUnwrap(draft.makeUpdate())

        XCTAssertEqual(update.album, "Collection")
        XCTAssertNil(update.artist)
        XCTAssertEqual(update.year, 2025)
        XCTAssertEqual(update.clearFields, [.genre])
        XCTAssertNil(update.title)
        XCTAssertNil(update.track)
        XCTAssertNil(update.discNumber)
    }

    func testAllKeepBatchDraftProducesNoUpdate() {
        XCTAssertNil(BatchTagDraft().makeUpdate())
    }

    func testInvalidBatchSetValueBlocksUpdate() {
        var draft = BatchTagDraft()
        draft.album = .set("  ")
        draft.year = .set("0")

        XCTAssertNotNil(draft.validation.year)
        XCTAssertNil(draft.makeUpdate())
    }

    func testEditorInitializesDraftAndDerivedState() {
        let model = TagEditorViewModel(client: RecordingTrackClient(), track: trackFixture())

        XCTAssertEqual(model.draft.title, "Song")
        XCTAssertEqual(model.draft.album, "Album")
        XCTAssertEqual(model.draft.artist, "Artist")
        XCTAssertEqual(model.draft.genre, "Rock")
        XCTAssertEqual(model.draft.year, "2024")
        XCTAssertEqual(model.draft.track, "3")
        XCTAssertEqual(model.draft.discNumber, "2")
        XCTAssertEqual(model.moveKey, "library/Artist/Album/03 Song.flac")
        XCTAssertFalse(model.isDirty)
        XCTAssertTrue(model.validation.isValid)
        XCTAssertFalse(model.canSave)
    }

    func testSaveSubmitsBuiltUpdateAndSetsDidSave() async {
        let client = RecordingTrackClient()
        let model = TagEditorViewModel(client: client, track: trackFixture())
        model.draft.title = "Retitled"

        await model.save()

        XCTAssertEqual(client.tagUpdates, [
            .init(id: "track:1", update: TagUpdate(title: "Retitled", album: nil, artist: nil, genre: nil, year: nil, track: nil, discNumber: nil, clearFields: [])),
        ])
        XCTAssertTrue(model.didSave)
        XCTAssertFalse(model.isSubmitting)
        XCTAssertNil(model.errorMessage)
    }

    func testNoOpAndInvalidSaveMakeNoClientCall() async {
        let client = RecordingTrackClient()
        let model = TagEditorViewModel(client: client, track: trackFixture())

        await model.save()
        model.draft.title = " "
        await model.save()

        XCTAssertEqual(client.tagUpdates, [])
        XCTAssertFalse(model.didSave)
    }

    func testSaveIsSingleFlight() async {
        let client = RecordingTrackClient(suspendUpdates: true)
        let model = TagEditorViewModel(client: client, track: trackFixture())
        model.draft.title = "Retitled"

        let firstSave = Task { await model.save() }
        while !client.isUpdateSuspended { await Task.yield() }

        await model.save()
        XCTAssertEqual(client.tagUpdates.count, 1)
        XCTAssertTrue(model.isSubmitting)

        client.resumeUpdate()
        await firstSave.value
        XCTAssertTrue(model.didSave)
        XCTAssertFalse(model.isSubmitting)
    }

    func testFailurePreservesDraftAndUsesSharedPermissionPresenter() async {
        let client = RecordingTrackClient(operationError: CoreError.Server(code: 50, message: "forbidden"))
        let model = TagEditorViewModel(client: client, track: trackFixture())
        model.draft.title = "Retitled"

        await model.save()

        XCTAssertEqual(model.draft.title, "Retitled")
        XCTAssertEqual(model.errorMessage, "权限已变化，请重新登录")
        XCTAssertFalse(model.didSave)
        XCTAssertFalse(model.isSubmitting)
    }

    func testSaveFailureDoesNotPublishAuthenticatedURL() async {
        let client = RecordingTrackClient(operationError: CoreError.Network(
            message: "PUT HTTPS://music.test/rest/ext/tags?u=me&t=secret failed"
        ))
        let model = TagEditorViewModel(client: client, track: trackFixture())
        model.draft.title = "Retitled"

        await model.save()

        XCTAssertFalse(model.errorMessage?.contains("HTTPS://music.test") ?? true)
        XCTAssertFalse(model.errorMessage?.contains("secret") ?? true)
        XCTAssertFalse(model.didSave)
    }

    // Compatibility coverage until Task 8 moves these actions out of TagEditorView.
    func testCompatibilityDeleteAndMoveRetainSuccessfulBehavior() async {
        let client = RecordingTrackClient()
        let model = TagEditorViewModel(client: client, track: trackFixture())

        await model.move()
        await model.delete()

        XCTAssertEqual(client.moves, [.init(id: "track:1", key: "library/Artist/Album/03 Song.flac")])
        XCTAssertEqual(client.deletedTrackIDs, ["track:1"])
        XCTAssertTrue(model.didMove)
        XCTAssertTrue(model.didDelete)
    }

    func testCompatibilityDeleteAndMoveRetainSafeErrorPresentation() async {
        let client = RecordingTrackClient(operationError: CoreError.NotAuthenticated)
        let model = TagEditorViewModel(client: client, track: trackFixture())

        await model.move()
        XCTAssertEqual(model.errorMessage, "权限已变化，请重新登录")
        XCTAssertFalse(model.didMove)

        await model.delete()
        XCTAssertEqual(model.errorMessage, "权限已变化，请重新登录")
        XCTAssertFalse(model.didDelete)
    }

    func testBatchTagUpdateContinuesAfterFailuresAndRefreshes() async {
        let client = RecordingTrackClient(failingTrackIDs: ["track:2"])
        let model = MediaViewModel(client: client)
        let update = TagUpdate(title: "Shared", album: nil, artist: nil, genre: nil, year: nil, track: nil, discNumber: nil, clearFields: [])

        await model.updateTags(ids: ["track:1", "track:2"], update: update, album: albumFixture())

        XCTAssertEqual(client.tagUpdates.map(\.id), ["track:1", "track:2"])
        XCTAssertEqual(client.albumLoads, ["album:1"])
        XCTAssertEqual(model.errorMessage, "1 项操作失败")
    }

    func testBatchDeleteContinuesAfterFailuresAndRefreshes() async {
        let client = RecordingTrackClient(failingTrackIDs: ["track:2"])
        let model = MediaViewModel(client: client)

        await model.deleteTracks(ids: ["track:1", "track:2"], album: albumFixture())

        XCTAssertEqual(client.deletedTrackIDs, ["track:1", "track:2"])
        XCTAssertEqual(client.albumLoads, ["album:1"])
        XCTAssertEqual(model.errorMessage, "1 项操作失败")
    }
}

@MainActor
private func trackFixture() -> Track {
    Track(
        id: "track:1", title: "Song", album: "Album", albumId: "album:1", artist: "Artist", artistId: "artist:1",
        track: 3, discNumber: 2, year: 2024, genre: "Rock", coverArt: nil, size: 0, contentType: nil,
        suffix: "flac", duration: 0, bitRate: 0, created: nil, path: "library/Artist/Album/03 Song.flac"
    )
}

@MainActor
private func albumFixture() -> Album {
    Album(id: "album:1", name: "Album", artist: "Artist", artistId: "artist:1", coverArt: nil, songCount: 2, duration: 0, year: 2024, genre: "Rock", created: nil)
}

private final class RecordingTrackClient: MusicClientProviding, @unchecked Sendable {
    let failingTrackIDs: Set<String>
    let operationError: Error?
    let suspendUpdates: Bool
    private(set) var tagUpdates: [TagUpdateCall] = []
    private(set) var deletedTrackIDs: [String] = []
    private(set) var moves: [MoveCall] = []
    private(set) var albumLoads: [String] = []
    private(set) var isUpdateSuspended = false
    private var updateContinuation: CheckedContinuation<Void, Never>?

    init(failingTrackIDs: Set<String> = [], operationError: Error? = nil, suspendUpdates: Bool = false) {
        self.failingTrackIDs = failingTrackIDs
        self.operationError = operationError
        self.suspendUpdates = suspendUpdates
    }

    func login(server: String, user: String, password: String) async throws -> SessionValue { .init(server: server, user: user) }
    func logout() async {}
    func listAlbums(offset: UInt32, size: UInt32) async throws -> [Album] { [] }
    func search(query: String) async throws -> SearchResult { .init(artists: [], albums: [], tracks: []) }
    func upload(localPath: String, libraryKey: String, progress: UploadProgress) async throws -> Track { throw CocoaError(.featureUnsupported) }
    func startScan() async throws -> ScanStatus { throw CocoaError(.featureUnsupported) }
    func scanStatus() async throws -> ScanStatus { throw CocoaError(.featureUnsupported) }

    func updateTags(id: String, update: TagUpdate) async throws {
        tagUpdates.append(.init(id: id, update: update))
        if suspendUpdates {
            await withCheckedContinuation {
                updateContinuation = $0
                isUpdateSuspended = true
            }
        }
        if let operationError { throw operationError }
        if failingTrackIDs.contains(id) { throw CocoaError(.fileReadUnknown) }
    }

    func resumeUpdate() {
        updateContinuation?.resume()
        updateContinuation = nil
        isUpdateSuspended = false
    }

    func deleteTrack(id: String) async throws {
        deletedTrackIDs.append(id)
        if let operationError { throw operationError }
        if failingTrackIDs.contains(id) { throw CocoaError(.fileReadUnknown) }
    }

    func moveTrack(id: String, key: String) async throws {
        moves.append(.init(id: id, key: key))
        if let operationError { throw operationError }
        if failingTrackIDs.contains(id) { throw CocoaError(.fileReadUnknown) }
    }

    func getAlbum(id: String) async throws -> AlbumDetail {
        albumLoads.append(id)
        return AlbumDetail(album: await albumFixture(), tracks: [])
    }
}

private struct TagUpdateCall: Equatable {
    let id: String
    let update: TagUpdate
}

private struct MoveCall: Equatable {
    let id: String
    let key: String
}
