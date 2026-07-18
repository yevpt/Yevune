import XCTest
@testable import Yevune

final class SearchTrackWorkbenchPolicyTests: XCTestCase {
    func testSelectedTracksFollowVisibleSearchOrder() {
        let tracks = [
            trackFixture(id: "track:1", title: "One"),
            trackFixture(id: "track:2", title: "Two"),
            trackFixture(id: "track:3", title: "Three"),
        ]

        XCTAssertEqual(
            SearchTrackWorkbenchPolicy.selectedTracks(tracks, selection: Set(["track:3", "track:1"]))
                .map(\.title),
            ["One", "Three"]
        )
    }

    func testRefreshDropsInvisibleSelectionAndSelectAllUsesCurrentResults() {
        let tracks = [
            trackFixture(id: "track:2", title: "Two"),
            trackFixture(id: "track:3", title: "Three"),
        ]

        XCTAssertEqual(
            SearchTrackWorkbenchPolicy.reconciledSelection(
                Set(["track:1", "track:2"]),
                tracks: tracks
            ),
            Set(["track:2"])
        )
        XCTAssertEqual(SearchTrackWorkbenchPolicy.selectAll(tracks), Set(["track:2", "track:3"]))
    }
}
