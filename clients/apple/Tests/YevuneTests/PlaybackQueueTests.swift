import XCTest
import AVFoundation
@testable import Yevune

final class PlaybackQueueTests: XCTestCase {
    func testReplaceStartsAtRequestedDuplicateInstance() {
        let repeated = playbackTrack("track:1")
        var queue = PlaybackQueue()
        queue.replace(with: [repeated, playbackTrack("track:2"), repeated], startingAt: 2)

        XCTAssertEqual(queue.entries.map(\.track.id), ["track:1", "track:2", "track:1"])
        XCTAssertEqual(queue.currentIndex, 2)
        XCTAssertEqual(Set(queue.entries.map(\.id)).count, 3)
    }

    func testPreviousAlwaysChangesToPreviousEntry() {
        var queue = PlaybackQueue()
        queue.replace(with: [playbackTrack("1"), playbackTrack("2")], startingAt: 1)

        XCTAssertEqual(queue.previous()?.track.id, "1")
        XCTAssertEqual(queue.currentIndex, 0)
    }

    func testInsertAppendMoveAndRemovePreserveInstances() {
        var queue = PlaybackQueue()
        queue.replace(with: [playbackTrack("1"), playbackTrack("2")], startingAt: 0)
        queue.insertNext(playbackTrack("3"))
        queue.append(playbackTrack("4"))
        queue.move(from: 3, to: 1)
        queue.remove(id: queue.entries[2].id)

        XCTAssertEqual(queue.entries.map(\.track.id), ["1", "4", "2"])
    }

    func testRepeatModesApplyOnlyAtNaturalEnd() {
        var queue = PlaybackQueue()
        queue.replace(with: [playbackTrack("1"), playbackTrack("2")], startingAt: 1)
        XCTAssertNil(queue.nextAfterNaturalEnd())

        queue.repeatMode = .all
        XCTAssertEqual(queue.nextAfterNaturalEnd()?.track.id, "1")
        queue.repeatMode = .one
        XCTAssertEqual(queue.nextAfterNaturalEnd()?.track.id, "1")
    }

    func testShuffleKeepsCurrentAndRestoresOriginalRemainingOrder() {
        var queue = PlaybackQueue()
        queue.replace(with: [playbackTrack("1"), playbackTrack("2"), playbackTrack("3")], startingAt: 0)
        queue.setShuffled(true) { Array($0.reversed()) }
        XCTAssertEqual(queue.entries.map(\.track.id), ["1", "3", "2"])
        XCTAssertEqual(queue.current?.track.id, "1")

        queue.setShuffled(false) { $0 }
        XCTAssertEqual(queue.entries.map(\.track.id), ["1", "2", "3"])
    }
}
