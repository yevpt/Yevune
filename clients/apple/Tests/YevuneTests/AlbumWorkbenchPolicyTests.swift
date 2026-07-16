import XCTest
import YevuneCoreFFI
@testable import Yevune

@MainActor
final class AlbumWorkbenchPolicyTests: XCTestCase {
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

    func testSelectionRefreshKeepsOnlyLoadedTrackIDs() {
        XCTAssertEqual(
            AlbumWorkbenchPolicy.reconciledSelection(
                ["kept", "removed"],
                tracks: [track(id: "kept"), track(id: "new")]
            ),
            ["kept"]
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
