import XCTest
import YevuneCoreFFI
@testable import Yevune

@MainActor
final class AlbumWorkbenchPolicyTests: XCTestCase {
    func testRootIntegrationStructurallyGuardsManagementSurfaces() throws {
        let mediaDetailSource = try source("Sources/Yevune/Views/MediaDetailView.swift")
        let libraryViewSource = try source("Sources/Yevune/Views/LibraryView.swift")
        let tagEditorSource = try source("Sources/Yevune/Views/TagEditorView.swift")

        XCTAssertTrue(mediaDetailSource.contains("if isAdmin"))
        XCTAssertFalse(mediaDetailSource.contains("Button(\"替换封面\") { importing = true }"))
        XCTAssertTrue(libraryViewSource.contains("onManageAccess: { accessTarget = $0 }"))
        XCTAssertFalse(tagEditorSource.contains("移动曲目"))
        XCTAssertFalse(tagEditorSource.contains("删除曲目"))
    }

    func testCompactInspectorUsesEssentialColumnsBelow620Points() {
        XCTAssertEqual(
            AlbumWorkbenchPolicy.columns(width: 480),
            [.trackNumber, .titleAndArtist, .duration]
        )
        XCTAssertEqual(
            AlbumWorkbenchPolicy.columns(width: 619.999),
            [.trackNumber, .titleAndArtist, .duration]
        )
    }

    func testWideDetailAddsArtistAndFormatColumnsAt620Points() {
        XCTAssertEqual(
            AlbumWorkbenchPolicy.columns(width: 620),
            [.trackNumber, .title, .artist, .duration, .format]
        )
        XCTAssertEqual(
            AlbumWorkbenchPolicy.columns(width: 720),
            [.trackNumber, .title, .artist, .duration, .format]
        )
    }

    func testHeaderAndTrackRowsShareOuterFrameWhileMetadataStaysBesideArtwork() {
        let compact = AlbumWorkbenchPolicy.gridMetrics(width: 480)
        let wide = AlbumWorkbenchPolicy.gridMetrics(width: 720)

        XCTAssertEqual(compact.outerHorizontalInset, 12)
        XCTAssertEqual(compact.trackGridLeadingEdge, compact.outerHorizontalInset)
        XCTAssertEqual(compact.contentTrailingEdge(width: 480), 468)
        XCTAssertEqual(compact.artworkSize, 144)
        XCTAssertEqual(compact.headerSpacing, 16)
        XCTAssertEqual(compact.metadataLeadingEdge, 172)

        XCTAssertEqual(wide.outerHorizontalInset, compact.outerHorizontalInset)
        XCTAssertEqual(wide.trackGridLeadingEdge, wide.outerHorizontalInset)
        XCTAssertEqual(wide.contentTrailingEdge(width: 720), 708)
        XCTAssertEqual(wide.artworkSize, 200)
        XCTAssertEqual(wide.headerSpacing, 24)
        XCTAssertEqual(wide.metadataLeadingEdge, 236)
        XCTAssertGreaterThan(compact.metadataLeadingEdge, compact.outerHorizontalInset + compact.artworkSize)
        XCTAssertGreaterThan(wide.metadataLeadingEdge, wide.outerHorizontalInset + wide.artworkSize)
    }

    func testMembersNeverReceiveManagementActions() {
        XCTAssertEqual(AlbumWorkbenchPolicy.managementActions(isAdmin: false), [])
        XCTAssertEqual(
            AlbumWorkbenchPolicy.managementActions(isAdmin: true),
            [.editTags, .replaceCover, .move, .delete, .manageAccess]
        )
    }

    func testMetadataJoinsOnlyPresentYearGenreSongCountAndLoadedDuration() {
        let tracks = [
            track(id: "one", duration: 61),
            track(id: "two", duration: 125),
        ]

        XCTAssertEqual(
            AlbumWorkbenchPolicy.metadata(
                album: album(year: 2026, genre: "Jazz", songCount: 8, duration: 9_999),
                tracks: tracks
            ),
            "2026 · Jazz · 8 首 · 3:06"
        )
    }

    func testMetadataOmitsMissingYearGenreAndZeroLoadedDuration() {
        XCTAssertEqual(
            AlbumWorkbenchPolicy.metadata(
                album: album(year: nil, genre: nil, songCount: 2),
                tracks: [track(id: "one", duration: 0)]
            ),
            "2 首"
        )
    }

    func testMetadataOmitsBlankGenre() {
        XCTAssertEqual(
            AlbumWorkbenchPolicy.metadata(
                album: album(year: 2026, genre: "  ", songCount: 1),
                tracks: [track(id: "one", duration: 60)]
            ),
            "2026 · 1 首 · 1:00"
        )
    }

    func testMultiDiscTrackNumberUsesDiscDotPaddedTrack() {
        XCTAssertEqual(
            AlbumWorkbenchPolicy.trackNumber(
                track(id: "one", discNumber: 1, trackNumber: 3),
                isMultiDisc: true
            ),
            "1·03"
        )
    }

    func testSingleDiscTrackNumberOmitsDiscAndPadsTrack() {
        XCTAssertEqual(
            AlbumWorkbenchPolicy.trackNumber(
                track(id: "one", discNumber: 1, trackNumber: 3),
                isMultiDisc: false
            ),
            "03"
        )
    }

    func testMissingTrackNumberUsesDash() {
        XCTAssertEqual(
            AlbumWorkbenchPolicy.trackNumber(
                track(id: "one", discNumber: nil, trackNumber: nil),
                isMultiDisc: true
            ),
            "—"
        )
    }

    func testMissingDiscNormalizesToDiscOneWithoutCreatingAnotherSection() {
        let explicit = track(id: "explicit", discNumber: 1, trackNumber: 2)
        let missing = track(id: "missing", discNumber: nil, trackNumber: 3)
        let tracks = [explicit, missing]

        XCTAssertEqual(AlbumWorkbenchPolicy.normalizedDiscNumber(missing), 1)
        XCTAssertFalse(AlbumWorkbenchPolicy.isMultiDisc(tracks))
        XCTAssertEqual(AlbumWorkbenchPolicy.discGroups(tracks).map(\.discNumber), [1])
        XCTAssertEqual(
            AlbumWorkbenchPolicy.discGroups(tracks).map { $0.tracks.map(\.id) },
            [["explicit", "missing"]]
        )
        XCTAssertEqual(
            AlbumWorkbenchPolicy.trackNumber(missing, isMultiDisc: false),
            "03"
        )
    }

    func testMissingAndSecondDiscUseNormalizedDiscForGroupingAndNumbering() {
        let missing = track(id: "missing", discNumber: nil, trackNumber: 3)
        let second = track(id: "second", discNumber: 2, trackNumber: 1)
        let tracks = [missing, second]

        XCTAssertTrue(AlbumWorkbenchPolicy.isMultiDisc(tracks))
        XCTAssertEqual(AlbumWorkbenchPolicy.discGroups(tracks).map(\.discNumber), [1, 2])
        XCTAssertEqual(
            AlbumWorkbenchPolicy.trackNumber(missing, isMultiDisc: true),
            "1·03"
        )
    }

    func testSelectionRefreshKeepsOnlyLoadedTrackIDs() {
        XCTAssertEqual(
            AlbumWorkbenchPolicy.reconciledSelection(
                ["kept", "removed"],
                tracks: [track(id: "kept"), track(id: "new")]
            ),
            ["kept"]
        )
    }

    func testInitialAppearanceDropsSelectionThatWasNeverLoaded() {
        XCTAssertEqual(
            AlbumWorkbenchPolicy.reconciledSelection(
                ["loaded", "stale"],
                trackIDs: ["loaded"]
            ),
            ["loaded"]
        )
    }

    func testRefreshToEmptyTracksClearsSelection() {
        XCTAssertEqual(
            AlbumWorkbenchPolicy.reconciledSelection(
                ["last-track"],
                trackIDs: []
            ),
            []
        )
    }

    func testMemberAndAdminEmptyMessagesAreDistinctAndActionable() {
        XCTAssertEqual(
            AlbumWorkbenchPolicy.emptyMessage(isAdmin: false),
            "此专辑暂无可播放的曲目。"
        )
        XCTAssertEqual(
            AlbumWorkbenchPolicy.emptyMessage(isAdmin: true),
            "此专辑暂无曲目，可通过曲库导入添加音乐。"
        )
    }

    private func source(_ relativePath: String) throws -> String {
        try String(contentsOf: packageRoot.appending(path: relativePath), encoding: .utf8)
    }

    private var packageRoot: URL {
        URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
    }
}

private func album(
    year: UInt32?,
    genre: String?,
    songCount: UInt32,
    duration: UInt32 = 0
) -> Album {
    Album(
        id: "album:1",
        name: "Album",
        artist: "Artist",
        artistId: "artist:1",
        coverArt: nil,
        songCount: songCount,
        duration: duration,
        year: year,
        genre: genre,
        created: nil
    )
}

private func track(
    id: String,
    duration: UInt32 = 0,
    discNumber: UInt32? = 1,
    trackNumber: UInt32? = 1
) -> Track {
    Track(
        id: id,
        title: id,
        album: "Album",
        albumId: "album:1",
        artist: "Artist",
        artistId: "artist:1",
        track: trackNumber,
        discNumber: discNumber,
        year: nil,
        genre: nil,
        coverArt: nil,
        size: 0,
        contentType: nil,
        suffix: "flac",
        duration: duration,
        bitRate: 0,
        created: nil,
        path: nil
    )
}
