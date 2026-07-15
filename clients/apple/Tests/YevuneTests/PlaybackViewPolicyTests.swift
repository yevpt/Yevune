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

    func testFocusedPageUsesOnlyIdentityLyricsAndTransportSections() {
        XCTAssertEqual(
            PlaybackViewPolicy.focusedPageSections,
            [.identity, .lyrics, .transport]
        )
    }

    func testUnavailableLyricsUsesExplicitChineseMessage() {
        XCTAssertEqual(LyricsState.unavailable.displayText, "暂无歌词")
    }

    func testEmptyQueueDismissesFocusAndDisablesTransport() {
        XCTAssertTrue(PlaybackViewPolicy.shouldDismissFocus(queueCount: 0))
        XCTAssertFalse(PlaybackViewPolicy.isTransportEnabled(queueCount: 0))
        XCTAssertFalse(PlaybackViewPolicy.shouldDismissFocus(queueCount: 1))
        XCTAssertTrue(PlaybackViewPolicy.isTransportEnabled(queueCount: 1))
    }

    func testFocusedLayoutSwitchesFromStackedToSplitAtWidthThreshold() {
        XCTAssertEqual(PlaybackViewPolicy.focusedLayout(forWidth: 899), .stacked)
        XCTAssertEqual(PlaybackViewPolicy.focusedLayout(forWidth: 900), .split)
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

    func testSliderUpperBoundIsFiniteAndSeekingDisabledForInvalidDurations() {
        let invalidDurations: [TimeInterval] = [.nan, .infinity, -.infinity, -1, 0]

        for duration in invalidDurations {
            XCTAssertFalse(PlaybackViewPolicy.canSeek(duration: duration))
            XCTAssertEqual(PlaybackViewPolicy.sliderUpperBound(duration: duration), 1)
        }

        XCTAssertTrue(PlaybackViewPolicy.canSeek(duration: 180))
        XCTAssertEqual(PlaybackViewPolicy.sliderUpperBound(duration: 180), 180)
    }

    func testSliderValueAlwaysClampsToFiniteSliderRange() {
        let abnormalElapsed: [TimeInterval] = [.nan, .infinity, -.infinity, -1]
        let abnormalDuration: [TimeInterval] = [.nan, .infinity, -.infinity, -1, 0]

        for elapsed in abnormalElapsed {
            XCTAssertEqual(PlaybackViewPolicy.sliderValue(elapsed: elapsed, duration: 180), 0)
        }
        for duration in abnormalDuration {
            let value = PlaybackViewPolicy.sliderValue(elapsed: 65, duration: duration)
            XCTAssertEqual(value, 0)
        }
        XCTAssertEqual(PlaybackViewPolicy.sliderValue(elapsed: 240, duration: 180), 180)
    }

    func testPlayerBarUsesCompactProductionLayoutAtMainWindowMinimumWidth() {
        XCTAssertEqual(PlaybackViewPolicy.playerBarLayout(forWidth: 920), .compact)
        XCTAssertEqual(PlaybackViewPolicy.playerBarLayout(forWidth: 1100), .regular)
    }

    func testFocusedStatusPrefersSafeErrorThenBufferingWithoutQueueLanguage() {
        XCTAssertEqual(
            PlaybackViewPolicy.focusedStatus(engineState: .buffering, errorMessage: nil),
            .buffering("正在缓冲")
        )
        XCTAssertEqual(
            PlaybackViewPolicy.focusedStatus(engineState: .buffering, errorMessage: "无法播放这首歌曲"),
            .error("无法播放这首歌曲")
        )
        XCTAssertFalse(String(describing: PlaybackViewPolicy.focusedStatus(engineState: .buffering, errorMessage: nil)).contains("接下来播放"))
    }

    func testMiniPlayerUsesExplicitEmptyBufferingAndErrorPresentations() {
        XCTAssertEqual(
            PlaybackViewPolicy.miniPlayerStatus(queueCount: 0, engineState: .idle, errorMessage: nil),
            .empty("播放队列为空")
        )
        XCTAssertEqual(
            PlaybackViewPolicy.miniPlayerStatus(queueCount: 1, engineState: .buffering, errorMessage: nil),
            .buffering("正在缓冲")
        )
        XCTAssertEqual(
            PlaybackViewPolicy.miniPlayerStatus(
                queueCount: 1,
                engineState: .buffering,
                errorMessage: "无法播放这首歌曲"
            ),
            .error("无法播放这首歌曲")
        )
    }

    func testKnownDurationProducesFormattedProgressAccessibilityValue() {
        XCTAssertNil(PlaybackViewPolicy.progressAccessibilityValue(elapsed: 65, duration: 0))
        XCTAssertEqual(
            PlaybackViewPolicy.progressAccessibilityValue(elapsed: 65, duration: 180),
            "1:05 / 3:00"
        )
    }
}
