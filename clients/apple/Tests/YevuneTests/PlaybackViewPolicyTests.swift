import XCTest
@testable import Yevune

final class PlaybackViewPolicyTests: XCTestCase {
    func testPlayerBarAppearsOnlyForNonEmptyQueue() {
        XCTAssertFalse(PlaybackViewPolicy.showsPlayerBar(queueCount: 0))
        XCTAssertTrue(PlaybackViewPolicy.showsPlayerBar(queueCount: 1))
    }

    func testAlbumContextSortsByDiscThenTrackAndPreservesStableUnknownOrder() {
        let tracks = [
            playbackTrack("b", disc: 2, number: 1),
            playbackTrack("a", disc: 1, number: 2),
            playbackTrack("c", disc: 1, number: 1),
            playbackTrack("unknown-1", disc: nil, number: nil),
            playbackTrack("unknown-2", disc: nil, number: nil),
        ]

        XCTAssertEqual(
            PlaybackViewPolicy.albumPlaybackOrder(tracks).map(\.id),
            ["c", "a", "b", "unknown-1", "unknown-2"]
        )
    }

    func testFocusedPageNeverExposesQueue() {
        XCTAssertFalse(PlaybackViewPolicy.focusedPageShowsQueue)
    }

    func testQueueClearIsEnabledOnlyWhenAnEntryFollowsCurrentInstance() {
        let first = UUID()
        let current = UUID()
        let upcoming = UUID()

        XCTAssertTrue(PlaybackViewPolicy.hasUpcomingQueueEntries(
            queueEntryIDs: [first, current, upcoming],
            currentID: current
        ))
        XCTAssertFalse(PlaybackViewPolicy.hasUpcomingQueueEntries(
            queueEntryIDs: [first, current],
            currentID: current
        ))
        XCTAssertFalse(PlaybackViewPolicy.hasUpcomingQueueEntries(
            queueEntryIDs: [first],
            currentID: nil
        ))
    }
}
