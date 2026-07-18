import XCTest
@testable import Yevune

final class TrackDragPayloadTests: XCTestCase {
    func testSelectedTrackPayloadUsesVisibleOrderInsteadOfSetOrder() {
        let payload = TrackDragPolicy.payload(
            rowTrackID: "track-3",
            selectedTrackIDs: ["track-3", "track-1"],
            orderedTrackIDs: ["track-1", "track-2", "track-3"]
        )

        XCTAssertEqual(payload.trackIDs, ["track-1", "track-3"])
    }

    func testDraggingAnUnselectedRowOnlyIncludesThatRow() {
        let payload = TrackDragPolicy.payload(
            rowTrackID: "track-2",
            selectedTrackIDs: ["track-1", "track-3"],
            orderedTrackIDs: ["track-1", "track-2", "track-3"]
        )

        XCTAssertEqual(payload.trackIDs, ["track-2"])
    }

    func testPositionPayloadPreservesRepeatedPlaylistInstances() {
        let payload = TrackDragPolicy.payload(
            rowPosition: 2,
            selectedPositions: [0, 2],
            orderedTrackIDs: ["repeat", "middle", "repeat"]
        )

        XCTAssertEqual(payload.trackIDs, ["repeat", "repeat"])
    }

    func testPositionPayloadFallsBackToValidDraggedRow() {
        XCTAssertEqual(
            TrackDragPolicy.payload(
                rowPosition: 1,
                selectedPositions: [0],
                orderedTrackIDs: ["first", "second"]
            ).trackIDs,
            ["second"]
        )
        XCTAssertTrue(
            TrackDragPolicy.payload(
                rowPosition: 9,
                selectedPositions: [],
                orderedTrackIDs: ["first"]
            ).trackIDs.isEmpty
        )
    }

    func testAcceptedDropMergesPayloadsWithoutRemovingRepeatedTracks() {
        let ids = TrackDragPolicy.acceptedTrackIDs(
            from: [
                TrackDragPayload(trackIDs: ["first", "repeat"]),
                TrackDragPayload(trackIDs: ["repeat", "last"]),
            ],
            isMutating: false
        )

        XCTAssertEqual(ids, ["first", "repeat", "repeat", "last"])
    }

    func testDropRejectsMutationAndEmptyIdentifiers() {
        XCTAssertNil(TrackDragPolicy.acceptedTrackIDs(
            from: [TrackDragPayload(trackIDs: ["track-1"])],
            isMutating: true
        ))
        XCTAssertNil(TrackDragPolicy.acceptedTrackIDs(
            from: [TrackDragPayload(trackIDs: ["", ""])],
            isMutating: false
        ))
    }
}
