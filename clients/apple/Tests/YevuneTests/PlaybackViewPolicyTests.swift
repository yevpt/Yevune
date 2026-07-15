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

    func testBufferingAndPlayingHaveDistinctPresentationsWhileBothPause() {
        let playing = PlaybackViewPolicy.transportPresentation(for: .playing)
        let buffering = PlaybackViewPolicy.transportPresentation(for: .buffering)

        XCTAssertEqual(playing.primaryAction, .pause)
        XCTAssertFalse(playing.showsBufferingIndicator)
        XCTAssertEqual(playing.primaryActionAccessibilityLabel, "暂停")

        XCTAssertEqual(buffering.primaryAction, .pause)
        XCTAssertTrue(buffering.showsBufferingIndicator)
        XCTAssertEqual(buffering.statusText, "正在缓冲")
        XCTAssertEqual(buffering.primaryActionAccessibilityLabel, "暂停（正在缓冲）")
    }

    func testNonEmptyPlaybackErrorProducesVisiblePresentation() {
        XCTAssertNil(PlaybackViewPolicy.errorPresentation(for: nil))
        XCTAssertNil(PlaybackViewPolicy.errorPresentation(for: ""))

        let presentation = PlaybackViewPolicy.errorPresentation(for: "无法播放这首歌曲")
        XCTAssertEqual(presentation?.message, "无法播放这首歌曲")
        XCTAssertEqual(presentation?.accessibilityLabel, "播放错误：无法播放这首歌曲")
    }

    func testSeekingIsDisabledUntilDurationIsKnown() {
        XCTAssertFalse(PlaybackViewPolicy.canSeek(duration: -1))
        XCTAssertFalse(PlaybackViewPolicy.canSeek(duration: 0))
        XCTAssertFalse(PlaybackViewPolicy.canSeek(duration: .infinity))
        XCTAssertTrue(PlaybackViewPolicy.canSeek(duration: 180))
    }

    func testKnownDurationProducesFormattedProgressAccessibilityValue() {
        XCTAssertNil(PlaybackViewPolicy.progressAccessibilityValue(elapsed: 65, duration: 0))
        XCTAssertEqual(
            PlaybackViewPolicy.progressAccessibilityValue(elapsed: 65, duration: 180),
            "1:05 / 3:00"
        )
    }
}
