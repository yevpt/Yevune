import XCTest
@testable import Yevune

final class PlaylistWorkbenchPolicyTests: XCTestCase {
    func testMovingUsesPositionIdentityAndPreservesRepeatedTracks() {
        let repeated = trackFixture(id: "track:1", title: "Repeat")
        let tracks = [
            repeated,
            trackFixture(id: "track:2", title: "Middle"),
            repeated,
            trackFixture(id: "track:3", title: "Tail"),
        ]

        let moved = PlaylistWorkbenchPolicy.moving(
            tracks,
            fromOffsets: IndexSet(integer: 0),
            toOffset: 3
        )

        XCTAssertEqual(moved.map(\.title), ["Middle", "Repeat", "Repeat", "Tail"])
        XCTAssertEqual(moved.filter { $0.id == "track:1" }.count, 2)
    }

    func testRemovingAndSelectingUseVisiblePlaylistOrder() {
        let tracks = [
            trackFixture(id: "track:1", title: "One"),
            trackFixture(id: "track:2", title: "Two"),
            trackFixture(id: "track:3", title: "Three"),
            trackFixture(id: "track:4", title: "Four"),
        ]

        XCTAssertEqual(
            PlaylistWorkbenchPolicy.removing(tracks, offsets: IndexSet([1, 3])).map(\.title),
            ["One", "Three"]
        )
        XCTAssertEqual(
            PlaylistWorkbenchPolicy.selectedTracks(tracks, positions: Set([2, 0])).map(\.title),
            ["One", "Three"]
        )
    }

    func testMetadataValidationRejectsBlankNameAndTrimsValues() {
        XCTAssertNil(PlaylistWorkbenchPolicy.metadata(name: "  \n ", comment: "note"))
        XCTAssertEqual(
            PlaylistWorkbenchPolicy.metadata(name: "  Road Trip  ", comment: "  Night drive  "),
            PlaylistMetadata(name: "Road Trip", comment: "Night drive")
        )
    }
}
