# macOS Modern Playback Shell Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace album-local preview playback with one native, application-wide macOS playback experience containing a queue, persistent player bar, focused now-playing page, system media integration, and mini player.

**Architecture:** Keep authentication and media URL generation in Rust `core`; keep audio output in the Apple client. A `@MainActor PlaybackController` owns a pure Swift `PlaybackQueue`, an `AVQueuePlayer` adapter, and a system media coordinator; every playback view observes that same controller.

**Tech Stack:** Swift 5.9, SwiftUI, AVFoundation, MediaPlayer, AppKit, XCTest, existing `YevuneCoreFFI`; no third-party dependencies.

## Global Constraints

- Follow `AGENTS.md`, ADR-0001, and `docs/superpowers/specs/2026-07-15-mac-playback-shell-design.md`.
- macOS deployment floor remains macOS 14.
- `core` generates authenticated stream/cover URLs; Swift owns platform playback and must never log complete authenticated URLs.
- The focused now-playing page must not show, overlay, or reserve space for “接下来播放”.
- The previous button always changes to the previous queue entry; it never restarts the current song based on elapsed time.
- Online streaming only: do not add offline downloads or a lyrics server/API in this plan.
- No third-party package or new Rust dependency.
- Product code follows red → green TDD; every task ends with a focused commit using `.agents/skills/git-commit/SKILL.md`.
- Before claiming completion run Swift tests/build, repository Rust test/clippy/fmt gates, and the macOS launcher smoke script.

## File Structure

Create focused files rather than enlarging `LibraryView.swift` or `MediaViewModel.swift`:

```text
clients/apple/Sources/Yevune/
├── Audio/
│   ├── PlaybackQueue.swift              # pure queue/order/repeat state
│   ├── PlaybackEngine.swift             # engine protocol and public engine events
│   ├── AVQueuePlaybackEngine.swift      # AVQueuePlayer adapter and observer cleanup
│   ├── PlaybackMediaResolver.swift      # core stream/cover URL bridge
│   ├── PlaybackArtworkLoader.swift      # authenticated cover bytes for system metadata
│   ├── PlaybackController.swift         # application-wide orchestration and recovery
│   └── SystemMediaCoordinator.swift     # media keys and MPNowPlayingInfoCenter
├── Views/Playback/
│   ├── PlayerBar.swift                  # persistent compact controls
│   ├── QueuePanel.swift                 # queue management outside focused page
│   ├── NowPlayingView.swift             # current-song-only focused page
│   └── MiniPlayerView.swift             # separate-window compact player
├── Views/LibraryView.swift              # inject controller and initiate playback
├── Views/MediaDetailView.swift          # album play actions; remove preview button
├── Views/PlaylistDetailView.swift       # playlist play actions
├── Model/MediaViewModel.swift            # metadata operations only after migration
├── Model/LoginViewModel.swift            # explicit logout event
└── App.swift                             # one controller + mini-player scene

clients/apple/Tests/YevuneTests/
├── PlaybackFixtures.swift
├── PlaybackQueueTests.swift
├── PlaybackEngineTests.swift
├── PlaybackControllerTests.swift
├── PlaybackViewPolicyTests.swift
└── SystemMediaCoordinatorTests.swift
```

---

### Task 1: Pure playback queue and deterministic ordering

**Files:**
- Create: `clients/apple/Sources/Yevune/Audio/PlaybackQueue.swift`
- Create: `clients/apple/Tests/YevuneTests/PlaybackFixtures.swift`
- Create: `clients/apple/Tests/YevuneTests/PlaybackQueueTests.swift`

**Interfaces:**
- Consumes: generated `YevuneCoreFFI.Track`.
- Produces: `QueueEntry`, `PlaybackRepeatMode`, and `PlaybackQueue` with `replace`, `insertNext`, `append`, `move`, `remove`, `previous`, `nextAfterManualSkip`, `nextAfterNaturalEnd`, and `setShuffled`.

- [ ] **Step 1: Add fixtures and failing queue tests**

Create a reusable fixture:

```swift
import YevuneCoreFFI

func playbackTrack(
    _ id: String,
    title: String? = nil,
    disc: UInt32? = 1,
    number: UInt32? = nil,
    duration: UInt32 = 180
) -> Track {
    Track(
        id: id, title: title ?? id, album: "Album", albumId: "album:1",
        artist: "Artist", artistId: "artist:1", track: number,
        discNumber: disc, year: 2026, genre: nil, coverArt: "cover:1",
        size: 0, contentType: "audio/flac", suffix: "flac",
        duration: duration, bitRate: 0, created: nil, path: nil
    )
}
```

Write tests that prove duplicate tracks remain separate queue instances and navigation is direct:

```swift
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
```

- [ ] **Step 2: Run the queue tests and confirm red**

Run: `swift test --package-path clients/apple --filter PlaybackQueueTests`

Expected: compile failure because `PlaybackQueue`, `PlaybackRepeatMode`, and `QueueEntry` do not exist.

- [ ] **Step 3: Implement the minimal pure queue model**

Implement these public shapes and keep all mutation inside `PlaybackQueue`:

```swift
import Foundation
import YevuneCoreFFI

struct QueueEntry: Identifiable {
    let id: UUID
    let track: Track

    init(id: UUID = UUID(), track: Track) {
        self.id = id
        self.track = track
    }
}

enum PlaybackRepeatMode: String, CaseIterable {
    case off, all, one
}

struct PlaybackQueue {
    private(set) var entries: [QueueEntry] = []
    private(set) var currentIndex: Int?
    private var originalEntries: [QueueEntry] = []
    private(set) var isShuffled = false
    var repeatMode: PlaybackRepeatMode = .off

    var current: QueueEntry? {
        guard let currentIndex, entries.indices.contains(currentIndex) else { return nil }
        return entries[currentIndex]
    }

    mutating func replace(with tracks: [Track], startingAt index: Int) {
        let newEntries = tracks.map { QueueEntry(track: $0) }
        entries = newEntries
        originalEntries = newEntries
        currentIndex = newEntries.indices.contains(index) ? index : newEntries.indices.first
        isShuffled = false
    }

    mutating func insertNext(_ track: Track) {
        let target = min((currentIndex ?? -1) + 1, entries.count)
        let entry = QueueEntry(track: track)
        entries.insert(entry, at: target)
        originalEntries.append(entry)
    }

    mutating func append(_ track: Track) {
        let entry = QueueEntry(track: track)
        entries.append(entry)
        originalEntries.append(entry)
        if currentIndex == nil { currentIndex = 0 }
    }

    mutating func previous() -> QueueEntry? {
        guard let currentIndex, currentIndex > 0 else { return nil }
        self.currentIndex = currentIndex - 1
        return current
    }

    mutating func nextAfterManualSkip() -> QueueEntry? {
        advance(wrap: repeatMode == .all)
    }

    mutating func nextAfterNaturalEnd() -> QueueEntry? {
        if repeatMode == .one { return current }
        return advance(wrap: repeatMode == .all)
    }
}
```

Add mutation helpers using `QueueEntry.id`, never `Track.id`:

```swift
mutating func move(from source: Int, to destination: Int) {
    guard entries.indices.contains(source), destination >= 0, destination < entries.count else { return }
    let currentID = current?.id
    let entry = entries.remove(at: source)
    entries.insert(entry, at: destination)
    currentIndex = currentID.flatMap { id in entries.firstIndex { $0.id == id } }
}

mutating func remove(id: UUID) {
    guard let index = entries.firstIndex(where: { $0.id == id }) else { return }
    let wasCurrent = index == currentIndex
    entries.remove(at: index)
    originalEntries.removeAll { $0.id == id }
    if entries.isEmpty { currentIndex = nil }
    else if wasCurrent { currentIndex = min(index, entries.count - 1) }
    else if let currentIndex, index < currentIndex { self.currentIndex = currentIndex - 1 }
}

mutating func setShuffled(_ enabled: Bool, using shuffle: ([QueueEntry]) -> [QueueEntry]) {
    guard enabled != isShuffled, let currentIndex else { return }
    let throughCurrent = Array(entries[...currentIndex])
    let used = Set(throughCurrent.map(\.id))
    let originalRemaining = originalEntries.filter { !used.contains($0.id) }
    entries = throughCurrent + (enabled ? shuffle(originalRemaining) : originalRemaining)
    self.currentIndex = throughCurrent.count - 1
    isShuffled = enabled
}

private mutating func advance(wrap: Bool) -> QueueEntry? {
    guard let currentIndex, !entries.isEmpty else { return nil }
    if currentIndex + 1 < entries.count { self.currentIndex = currentIndex + 1 }
    else if wrap { self.currentIndex = 0 }
    else { return nil }
    return current
}
```

- [ ] **Step 4: Run focused and complete Swift tests**

Run: `swift test --package-path clients/apple --filter PlaybackQueueTests`

Expected: all `PlaybackQueueTests` pass.

Run: `swift test --package-path clients/apple`

Expected: existing Swift suite remains green.

- [ ] **Step 5: Commit the queue slice**

```bash
git add clients/apple/Sources/Yevune/Audio/PlaybackQueue.swift \
  clients/apple/Tests/YevuneTests/PlaybackFixtures.swift \
  clients/apple/Tests/YevuneTests/PlaybackQueueTests.swift
git commit -m "feat(mac): 建立全局播放队列模型"
```

---

### Task 2: AVQueuePlayer engine adapter and observer lifecycle

**Files:**
- Create: `clients/apple/Sources/Yevune/Audio/PlaybackEngine.swift`
- Create: `clients/apple/Sources/Yevune/Audio/AVQueuePlaybackEngine.swift`
- Create: `clients/apple/Tests/YevuneTests/PlaybackEngineTests.swift`

**Interfaces:**
- Consumes: `URL`, AVFoundation player/item notifications.
- Produces: `PlaybackEngine`, `PlaybackEngineEvent`, `PlaybackEngineState`, and `AVQueuePlaybackEngine`.

- [ ] **Step 1: Write failing tests for event mapping and cleanup**

Define tests against an injectable player surface so no network audio is needed:

```swift
import XCTest
@testable import Yevune

@MainActor
final class PlaybackEngineTests: XCTestCase {
    func testLoadReplacesItemAndHonorsAutoplay() {
        let player = FakeQueuePlayerSurface()
        let engine = AVQueuePlaybackEngine(player: player)

        engine.load(url: URL(string: "https://example.invalid/song")!, autoplay: true)

        XCTAssertEqual(player.loadedURL?.absoluteString, "https://example.invalid/song")
        XCTAssertEqual(player.playCalls, 1)
    }

    func testStopRemovesTimeObserverAndCurrentItem() {
        let player = FakeQueuePlayerSurface()
        let engine = AVQueuePlaybackEngine(player: player)
        engine.load(url: URL(string: "https://example.invalid/song")!, autoplay: false)

        engine.stop()

        XCTAssertEqual(player.removeTimeObserverCalls, 1)
        XCTAssertNil(player.loadedURL)
    }

    func testSeekAndVolumeClampAtEngineBoundary() {
        let player = FakeQueuePlayerSurface()
        let engine = AVQueuePlaybackEngine(player: player)
        engine.seek(to: -4)
        engine.setVolume(1.5)

        XCTAssertEqual(player.lastSeek, 0)
        XCTAssertEqual(player.volume, 1)
    }

    func testItemNotificationsBecomeEndedBufferingAndFailedEvents() {
        let center = NotificationCenter()
        let player = FakeQueuePlayerSurface()
        let engine = AVQueuePlaybackEngine(player: player, notificationCenter: center)
        var events: [PlaybackEngineEvent] = []
        engine.onEvent = { events.append($0) }
        engine.load(url: URL(string: "https://example.invalid/song")!, autoplay: false)
        let item = try! XCTUnwrap(player.currentItem)

        center.post(name: .AVPlayerItemPlaybackStalled, object: item)
        center.post(name: .AVPlayerItemDidPlayToEndTime, object: item)
        center.post(
            name: .AVPlayerItemFailedToPlayToEndTime,
            object: item,
            userInfo: [AVPlayerItemFailedToPlayToEndTimeErrorKey: CocoaError(.fileReadUnknown)]
        )

        XCTAssertEqual(events[0], .state(.buffering))
        XCTAssertEqual(events[1], .ended)
        guard case .failed = events[2] else { return XCTFail("expected failed event") }
    }
}

@MainActor
private final class FakeQueuePlayerSurface: QueuePlayerSurface {
    var volume: Float = 1
    var isMuted = false
    private(set) var currentItem: AVPlayerItem?
    private(set) var playCalls = 0
    private(set) var pauseCalls = 0
    private(set) var lastSeek: TimeInterval?
    private(set) var removeTimeObserverCalls = 0

    var loadedURL: URL? { (currentItem?.asset as? AVURLAsset)?.url }

    func replaceCurrentItem(with item: AVPlayerItem?) { currentItem = item }
    func play() { playCalls += 1 }
    func pause() { pauseCalls += 1 }
    func seek(to time: CMTime) { lastSeek = time.seconds }
    func addPeriodicTimeObserver(
        forInterval interval: CMTime,
        queue: DispatchQueue?,
        using block: @escaping @Sendable (CMTime) -> Void
    ) -> Any { NSObject() }
    func removeTimeObserver(_ observer: Any) { removeTimeObserverCalls += 1 }
}
```

- [ ] **Step 2: Run and confirm red**

Run: `swift test --package-path clients/apple --filter PlaybackEngineTests`

Expected: compile failure because the engine protocol and adapter do not exist.

- [ ] **Step 3: Define the engine boundary**

```swift
import Foundation

enum PlaybackEngineState: Equatable {
    case idle, paused, playing, buffering
}

enum PlaybackEngineEvent: Equatable {
    case state(PlaybackEngineState)
    case time(elapsed: TimeInterval, duration: TimeInterval)
    case ended
    case failed(message: String)
}

@MainActor
protocol PlaybackEngine: AnyObject {
    var onEvent: ((PlaybackEngineEvent) -> Void)? { get set }
    func load(url: URL, autoplay: Bool)
    func play()
    func pause()
    func seek(to seconds: TimeInterval)
    func setVolume(_ volume: Float)
    func setMuted(_ muted: Bool)
    func stop()
}
```

- [ ] **Step 4: Implement `AVQueuePlaybackEngine`**

Use this small internal player surface, implemented directly by `AVQueuePlayer`:

```swift
import AVFoundation

@MainActor
protocol QueuePlayerSurface: AnyObject {
    var volume: Float { get set }
    var isMuted: Bool { get set }
    var currentItem: AVPlayerItem? { get }
    func replaceCurrentItem(with item: AVPlayerItem?)
    func play()
    func pause()
    func seek(to time: CMTime)
    func addPeriodicTimeObserver(
        forInterval interval: CMTime,
        queue: DispatchQueue?,
        using block: @escaping @Sendable (CMTime) -> Void
    ) -> Any
    func removeTimeObserver(_ observer: Any)
}

extension AVQueuePlayer: QueuePlayerSurface {}
```

The adapter then owns observer installation and removal:

```swift
@MainActor
final class AVQueuePlaybackEngine: PlaybackEngine {
    var onEvent: ((PlaybackEngineEvent) -> Void)?
    private let player: any QueuePlayerSurface
    private let notificationCenter: NotificationCenter
    private var timeObserver: Any?
    private var itemObservers: [NSObjectProtocol] = []

    init(
        player: any QueuePlayerSurface = AVQueuePlayer(),
        notificationCenter: NotificationCenter = .default
    ) {
        self.player = player
        self.notificationCenter = notificationCenter
    }

    func load(url: URL, autoplay: Bool) {
        removeObservers()
        player.replaceCurrentItem(with: AVPlayerItem(url: url))
        installObservers()
        if autoplay { player.play() }
    }

    func stop() {
        player.pause()
        removeObservers()
        player.replaceCurrentItem(with: nil)
        onEvent?(.state(.idle))
    }
}
```

Map `timeControlStatus` to `.paused`, `.playing`, or `.buffering`; map the periodic observer to finite nonnegative elapsed/duration values; observe item end and failure notifications. `removeObservers()` must remove every notification token and the periodic observer exactly once.

- [ ] **Step 5: Run tests and build**

Run: `swift test --package-path clients/apple --filter PlaybackEngineTests`

Expected: all engine tests pass.

Run: `swift build --package-path clients/apple`

Expected: build succeeds without concurrency warnings promoted to errors.

- [ ] **Step 6: Commit the engine slice**

```bash
git add clients/apple/Sources/Yevune/Audio/PlaybackEngine.swift \
  clients/apple/Sources/Yevune/Audio/AVQueuePlaybackEngine.swift \
  clients/apple/Tests/YevuneTests/PlaybackEngineTests.swift
git commit -m "feat(mac): 接入原生队列播放引擎"
```

---

### Task 3: Global controller, media resolver, and basic navigation

**Files:**
- Create: `clients/apple/Sources/Yevune/Audio/PlaybackMediaResolver.swift`
- Create: `clients/apple/Sources/Yevune/Audio/PlaybackController.swift`
- Create: `clients/apple/Tests/YevuneTests/PlaybackControllerTests.swift`
- Modify: `clients/apple/Sources/Yevune/Model/MediaViewModel.swift`
- Modify: `clients/apple/Tests/YevuneTests/TagEditorViewModelTests.swift`

**Interfaces:**
- Consumes: `PlaybackQueue`, `PlaybackEngine`, `MusicClientProviding.streamURL`, and `coverArtURL`.
- Produces: `ResolvedPlaybackMedia`, `PlaybackMediaResolving`, `MusicClientMediaResolver`, and application-wide `PlaybackController` intents.

- [ ] **Step 1: Write failing controller tests**

Create fakes that record resolved track IDs and engine loads, then test one global session:

```swift
@MainActor
final class PlaybackControllerTests: XCTestCase {
    func testPlayLoadsRequestedTrackAndPublishesQueue() async {
        let engine = RecordingPlaybackEngine()
        let resolver = RecordingMediaResolver()
        let controller = PlaybackController(resolver: resolver, engine: engine)

        await controller.play(
            tracks: [playbackTrack("1"), playbackTrack("2")],
            startingAt: 1
        )

        XCTAssertEqual(controller.currentTrack?.id, "2")
        XCTAssertEqual(controller.queueEntries.map(\.track.id), ["1", "2"])
        XCTAssertEqual(resolver.resolvedTrackIDs, ["2"])
        XCTAssertEqual(engine.loadedURLs.map(\.lastPathComponent), ["2"])
    }

    func testPreviousAndNextLoadAdjacentEntriesDirectly() async {
        let engine = RecordingPlaybackEngine()
        let controller = PlaybackController(resolver: RecordingMediaResolver(), engine: engine)
        await controller.play(tracks: [playbackTrack("1"), playbackTrack("2")], startingAt: 1)

        await controller.previous()
        await controller.next()

        XCTAssertEqual(engine.loadedURLs.map(\.lastPathComponent), ["2", "1", "2"])
    }

    func testEngineEventsPublishPlaybackStateAndProgress() async {
        let engine = RecordingPlaybackEngine()
        let controller = PlaybackController(resolver: RecordingMediaResolver(), engine: engine)
        await controller.play(tracks: [playbackTrack("1")], startingAt: 0)

        engine.send(.state(.buffering))
        engine.send(.time(elapsed: 12, duration: 180))

        XCTAssertEqual(controller.engineState, .buffering)
        XCTAssertEqual(controller.elapsed, 12)
        XCTAssertEqual(controller.duration, 180)
    }
}

@MainActor
final class RecordingPlaybackEngine: PlaybackEngine {
    var onEvent: ((PlaybackEngineEvent) -> Void)?
    private(set) var loadedURLs: [URL] = []
    private(set) var stopCalls = 0
    func load(url: URL, autoplay: Bool) { loadedURLs.append(url) }
    func play() {}
    func pause() {}
    func seek(to seconds: TimeInterval) {}
    func setVolume(_ volume: Float) {}
    func setMuted(_ muted: Bool) {}
    func stop() { stopCalls += 1 }
    func send(_ event: PlaybackEngineEvent) { onEvent?(event) }
}

@MainActor
final class RecordingMediaResolver: PlaybackMediaResolving {
    private(set) var resolvedTrackIDs: [String] = []
    func resolve(track: Track) async throws -> ResolvedPlaybackMedia {
        resolvedTrackIDs.append(track.id)
        return ResolvedPlaybackMedia(
            streamURL: URL(string: "https://example.invalid/\(track.id)")!,
            coverURL: URL(string: "https://example.invalid/cover-\(track.id)")
        )
    }
}
```

- [ ] **Step 2: Run and confirm red**

Run: `swift test --package-path clients/apple --filter PlaybackControllerTests`

Expected: compile failure because the resolver and controller types do not exist.

- [ ] **Step 3: Implement media resolution without logging URLs**

```swift
struct ResolvedPlaybackMedia: Equatable {
    let streamURL: URL
    let coverURL: URL?
}

enum PlaybackError: LocalizedError {
    case invalidMediaURL
    var errorDescription: String? { "服务器返回了无效的播放地址" }
}

@MainActor
protocol PlaybackMediaResolving: AnyObject {
    func resolve(track: Track) async throws -> ResolvedPlaybackMedia
}

@MainActor
final class MusicClientMediaResolver: PlaybackMediaResolving {
    let client: any MusicClientProviding

    init(client: any MusicClientProviding) { self.client = client }

    func resolve(track: Track) async throws -> ResolvedPlaybackMedia {
        let stream = try await client.streamURL(trackID: track.id)
        guard let streamURL = URL(string: stream) else { throw PlaybackError.invalidMediaURL }
        let coverURL: URL?
        if let coverArt = track.coverArt,
           let coverString = try? await client.coverArtURL(id: coverArt, size: 600) {
            coverURL = URL(string: coverString)
        } else {
            coverURL = nil
        }
        return ResolvedPlaybackMedia(streamURL: streamURL, coverURL: coverURL)
    }
}
```

Error descriptions may include the track title but never the URL.

- [ ] **Step 4: Implement the basic `PlaybackController`**

Expose the exact UI-facing state and intents:

```swift
@MainActor
final class PlaybackController: ObservableObject {
    @Published private(set) var queueEntries: [QueueEntry] = []
    @Published private(set) var currentTrack: Track?
    @Published private(set) var coverURL: URL?
    @Published private(set) var engineState: PlaybackEngineState = .idle
    @Published private(set) var elapsed: TimeInterval = 0
    @Published private(set) var duration: TimeInterval = 0
    @Published private(set) var errorMessage: String?
    @Published private(set) var isMuted = false
    @Published private(set) var volume: Float = 1

    private let resolver: any PlaybackMediaResolving
    private let engine: any PlaybackEngine
    private let shuffle: ([QueueEntry]) -> [QueueEntry]

    init(
        resolver: any PlaybackMediaResolving,
        engine: any PlaybackEngine,
        shuffle: @escaping ([QueueEntry]) -> [QueueEntry] = { $0.shuffled() }
    ) {
        self.resolver = resolver
        self.engine = engine
        self.shuffle = shuffle
        engine.onEvent = { [weak self] event in self?.handle(event) }
    }

    func play(tracks: [Track], startingAt index: Int) async
    func playNow(_ track: Track) async
    func playNext(_ track: Track)
    func addToQueue(_ track: Track)
    func togglePlayPause()
    func previous() async
    func next() async
    func seek(to seconds: TimeInterval)
    func setVolume(_ value: Float)
    func toggleMuted()
    func shutdown()
}
```

Install one engine event closure in `init`. On `.ended`, start a main-actor `Task` that advances using `nextAfterNaturalEnd`; on manual previous/next use the queue’s direct navigation methods. Synchronize published queue/current state after every mutation.

- [ ] **Step 5: Remove album-local AVPlayer from `MediaViewModel`**

Delete `import AVFoundation`, `playingTrackID`, the private player, and `toggle(track:)`. Keep metadata load/update/delete/cover responsibilities unchanged. Update the existing tests only where compilation requires removal of playback assumptions; do not weaken tag/delete assertions.

- [ ] **Step 6: Run controller and regression tests**

Run: `swift test --package-path clients/apple --filter PlaybackControllerTests`

Expected: controller tests pass.

Run: `swift test --package-path clients/apple`

Expected: entire Swift suite passes after local preview removal.

- [ ] **Step 7: Commit the controller slice**

```bash
git add clients/apple/Sources/Yevune/Audio/PlaybackMediaResolver.swift \
  clients/apple/Sources/Yevune/Audio/PlaybackController.swift \
  clients/apple/Sources/Yevune/Model/MediaViewModel.swift \
  clients/apple/Tests/YevuneTests/PlaybackControllerTests.swift \
  clients/apple/Tests/YevuneTests/TagEditorViewModelTests.swift
git commit -m "feat(mac): 统一应用播放状态"
```

---

### Task 4: Queue controls, repeat/shuffle, and bounded failure recovery

**Files:**
- Modify: `clients/apple/Sources/Yevune/Audio/PlaybackController.swift`
- Modify: `clients/apple/Sources/Yevune/Audio/PlaybackQueue.swift`
- Modify: `clients/apple/Tests/YevuneTests/PlaybackControllerTests.swift`
- Modify: `clients/apple/Tests/YevuneTests/PlaybackQueueTests.swift`

**Interfaces:**
- Consumes: Task 1 queue mutations and Task 3 controller.
- Produces: queue edit intents, `setShuffled`, `cycleRepeatMode`, retry-once recovery, and failed-entry cycle protection.

- [ ] **Step 1: Add failing tests for user controls and recovery**

```swift
func testFailureRefreshesURLOnceThenSkipsBadEntry() async {
    let engine = RecordingPlaybackEngine()
    let resolver = RecordingMediaResolver()
    let controller = PlaybackController(resolver: resolver, engine: engine)
    await controller.play(tracks: [playbackTrack("bad"), playbackTrack("good")], startingAt: 0)

    engine.send(.failed(message: "expired"))
    await controller.waitForPendingTransitionForTesting()
    engine.send(.failed(message: "still bad"))
    await controller.waitForPendingTransitionForTesting()

    XCTAssertEqual(resolver.resolvedTrackIDs, ["bad", "bad", "good"])
    XCTAssertEqual(controller.currentTrack?.id, "good")
    XCTAssertEqual(controller.errorMessage, "无法播放 bad，已跳到下一首")
}

func testFailedEntryIsNotRetriedForeverWhenRepeatAllWraps() async {
    let engine = RecordingPlaybackEngine()
    let resolver = RecordingMediaResolver()
    let controller = PlaybackController(resolver: resolver, engine: engine)
    await controller.play(tracks: [playbackTrack("a"), playbackTrack("b")], startingAt: 0)
    controller.cycleRepeatMode()

    for _ in 0..<4 {
        engine.send(.failed(message: "broken"))
        await controller.waitForPendingTransitionForTesting()
    }

    XCTAssertEqual(resolver.resolvedTrackIDs, ["a", "a", "b", "b"])
    XCTAssertEqual(engine.stopCalls, 1)
}

func testShuffleKeepsCurrentTrackAndRepeatCycleIsOffAllOne() async {
    let controller = PlaybackController(
        resolver: RecordingMediaResolver(),
        engine: RecordingPlaybackEngine(),
        shuffle: { Array($0.reversed()) }
    )
    await controller.play(
        tracks: [playbackTrack("1"), playbackTrack("2"), playbackTrack("3")],
        startingAt: 0
    )

    controller.setShuffled(true)
    XCTAssertEqual(controller.currentTrack?.id, "1")
    XCTAssertEqual(controller.queueEntries.map(\.track.id), ["1", "3", "2"])
    XCTAssertEqual(controller.repeatMode, .off)
    controller.cycleRepeatMode()
    XCTAssertEqual(controller.repeatMode, .all)
    controller.cycleRepeatMode()
    XCTAssertEqual(controller.repeatMode, .one)
}
```

- [ ] **Step 2: Run and confirm red**

Run: `swift test --package-path clients/apple --filter PlaybackControllerTests`

Expected: failures because recovery synchronization and advanced intents do not exist.

- [ ] **Step 3: Implement queue and mode intents**

Add:

```swift
@Published private(set) var isShuffled = false
@Published private(set) var repeatMode: PlaybackRepeatMode = .off

func removeFromQueue(id: UUID)
func moveQueueEntry(from: Int, to: Int)
func clearUpcoming()
func setShuffled(_ enabled: Bool)
func cycleRepeatMode()
```

Inject a `shuffle: ([QueueEntry]) -> [QueueEntry]` closure into the controller, defaulting to `{ $0.shuffled() }`, so tests are deterministic. `clearUpcoming` keeps the current entry and removes only later entries.

- [ ] **Step 4: Implement bounded media failure recovery**

Track attempts by `QueueEntry.id`:

```swift
private var retryCounts: [UUID: Int] = [:]
private var failedInCycle: Set<UUID> = []

private func recoverFromFailure(for entry: QueueEntry) async {
    if retryCounts[entry.id, default: 0] == 0 {
        retryCounts[entry.id] = 1
        await loadCurrent(autoplay: true)
        return
    }
    failedInCycle.insert(entry.id)
    errorMessage = "无法播放 \(entry.track.title)，已跳到下一首"
    await advancePastFailedEntries()
}
```

`advancePastFailedEntries` may visit each current queue instance at most once. When every candidate is in `failedInCycle`, stop the engine and retain the queue/current selection for manual retry. A user-initiated `play`, `previous`, `next`, or direct queue selection clears that entry’s retry/failure state.

Use an internal transition `Task` handle and a test-only `waitForPendingTransitionForTesting()` method compiled into the module so async event assertions do not use sleeps.

- [ ] **Step 5: Run focused tests repeatedly**

Run: `for i in {1..20}; do swift test --package-path clients/apple --filter PlaybackControllerTests || exit 1; done`

Expected: all 20 runs pass; no race-dependent failures.

Run: `swift test --package-path clients/apple --filter PlaybackQueueTests`

Expected: all queue tests pass.

- [ ] **Step 6: Commit advanced behavior**

```bash
git add clients/apple/Sources/Yevune/Audio/PlaybackController.swift \
  clients/apple/Sources/Yevune/Audio/PlaybackQueue.swift \
  clients/apple/Tests/YevuneTests/PlaybackControllerTests.swift \
  clients/apple/Tests/YevuneTests/PlaybackQueueTests.swift
git commit -m "feat(mac): 完善播放队列与失败恢复"
```

---

### Task 5: Album/playlist entry points, persistent player bar, and queue panel

**Files:**
- Create: `clients/apple/Sources/Yevune/Views/Playback/PlayerBar.swift`
- Create: `clients/apple/Sources/Yevune/Views/Playback/QueuePanel.swift`
- Create: `clients/apple/Tests/YevuneTests/PlaybackViewPolicyTests.swift`
- Modify: `clients/apple/Sources/Yevune/Views/LibraryView.swift`
- Modify: `clients/apple/Sources/Yevune/Views/MediaDetailView.swift`
- Modify: `clients/apple/Sources/Yevune/Views/PlaylistDetailView.swift`
- Modify: `clients/apple/Sources/Yevune/App.swift`

**Interfaces:**
- Consumes: one `PlaybackController` created by `YevuneApp`.
- Produces: reusable album/playlist playback commands, persistent bottom bar, and queue editor outside the focused page.

- [ ] **Step 1: Add failing pure view-policy tests**

Extract testable display/action rules rather than snapshot-testing SwiftUI internals:

```swift
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
        ]
        XCTAssertEqual(PlaybackViewPolicy.albumPlaybackOrder(tracks).map(\.id), ["c", "a", "b"])
    }

    func testFocusedPageNeverExposesQueue() {
        XCTAssertFalse(PlaybackViewPolicy.focusedPageShowsQueue)
    }
}
```

- [ ] **Step 2: Run and confirm red**

Run: `swift test --package-path clients/apple --filter PlaybackViewPolicyTests`

Expected: compile failure because `PlaybackViewPolicy` does not exist.

- [ ] **Step 3: Create and inject one controller**

Add the tested policy next to the playback views:

```swift
enum PlaybackViewPolicy {
    static func showsPlayerBar(queueCount: Int) -> Bool { queueCount > 0 }
    static let focusedPageShowsQueue = false

    static func albumPlaybackOrder(_ tracks: [Track]) -> [Track] {
        tracks.enumerated().sorted { left, right in
            let leftDisc = left.element.discNumber ?? .max
            let rightDisc = right.element.discNumber ?? .max
            if leftDisc != rightDisc { return leftDisc < rightDisc }
            let leftTrack = left.element.track ?? .max
            let rightTrack = right.element.track ?? .max
            if leftTrack != rightTrack { return leftTrack < rightTrack }
            return left.offset < right.offset
        }.map(\.element)
    }
}
```

In `YevuneApp.init`, construct the controller from the same `CoreMusicClient` used for login and library:

```swift
let client = CoreMusicClient()
_login = StateObject(wrappedValue: LoginViewModel(client: client))
_library = StateObject(wrappedValue: LibraryViewModel(client: client))
_playback = StateObject(wrappedValue: PlaybackController(
    resolver: MusicClientMediaResolver(client: client),
    engine: AVQueuePlaybackEngine()
))
```

Pass `playback` into `LibraryView`; do not create it inside album or playlist views.

- [ ] **Step 4: Replace “试听” with context-aware playback**

In `MediaDetailView`, add `@ObservedObject var playback: PlaybackController` and replace each local toggle button with:

```swift
Button {
    let ordered = PlaybackViewPolicy.albumPlaybackOrder(detail.tracks)
    let index = ordered.firstIndex { $0.id == track.id } ?? 0
    Task { await playback.play(tracks: ordered, startingAt: index) }
} label: {
    Image(systemName: playback.currentTrack?.id == track.id && playback.engineState == .playing
          ? "speaker.wave.2.fill" : "play.fill")
}
```

Add context actions with exact copy: `立即播放`, `下一首播放`, `加入队列`. In `PlaylistDetailView`, preserve `detail.tracks` order and duplicate positions; use the enumerated index rather than searching by track ID.

In `LibraryView` search results, render `result.tracks` as playable rows in addition to album matches. Double-clicking a search track calls:

```swift
let index = result.tracks.indices.contains(clickedIndex) ? clickedIndex : 0
Task { await playback.play(tracks: result.tracks, startingAt: index) }
```

The search row context menu exposes the same three actions. Preserve server search-result order; do not sort or deduplicate it.

- [ ] **Step 5: Implement `PlayerBar` and `QueuePanel`**

`PlayerBar` contains:

```swift
struct PlayerBar: View {
    @ObservedObject var playback: PlaybackController
    let openNowPlaying: () -> Void
    @State private var queuePresented = false

    var body: some View {
        HStack {
            CurrentTrackSummary(playback: playback, action: openNowPlaying)
            Spacer()
            TransportControls(playback: playback)
            Spacer()
            PlaybackOptions(playback: playback, queuePresented: $queuePresented)
        }
        .frame(minHeight: 72)
        .background(.regularMaterial)
        .popover(isPresented: $queuePresented) { QueuePanel(playback: playback) }
    }
}
```

Use a `Slider` with a local drag value so periodic progress updates do not fight the pointer. `QueuePanel` lists queue instance IDs, marks the current instance, and calls controller move/remove/clear intents. It must not appear inside `NowPlayingView`.

- [ ] **Step 6: Integrate bottom safe areas without covering the task drawer**

Make the bottom inset a single vertical stack in `LibraryView`: task drawer first when open, then `PlayerBar` when `PlaybackViewPolicy.showsPlayerBar` is true. Delete the old local play state references.

- [ ] **Step 7: Run tests and compile the integrated UI**

Run: `swift test --package-path clients/apple --filter PlaybackViewPolicyTests`

Expected: all policy tests pass.

Run: `swift test --package-path clients/apple`

Expected: all Swift tests pass.

Run: `swift build --package-path clients/apple`

Expected: app builds with all playback entry points.

- [ ] **Step 8: Commit the first visible playback shell**

```bash
git add clients/apple/Sources/Yevune/App.swift \
  clients/apple/Sources/Yevune/Views/LibraryView.swift \
  clients/apple/Sources/Yevune/Views/MediaDetailView.swift \
  clients/apple/Sources/Yevune/Views/PlaylistDetailView.swift \
  clients/apple/Sources/Yevune/Views/Playback \
  clients/apple/Tests/YevuneTests/PlaybackViewPolicyTests.swift
git commit -m "feat(mac): 接入全局播放栏与队列面板"
```

---

### Task 6: Focused current-song page and mini player window

**Files:**
- Create: `clients/apple/Sources/Yevune/Views/Playback/NowPlayingView.swift`
- Create: `clients/apple/Sources/Yevune/Views/Playback/MiniPlayerView.swift`
- Modify: `clients/apple/Sources/Yevune/Views/LibraryView.swift`
- Modify: `clients/apple/Sources/Yevune/App.swift`
- Modify: `clients/apple/Tests/YevuneTests/PlaybackViewPolicyTests.swift`

**Interfaces:**
- Consumes: Task 5 `PlayerBar.openNowPlaying` and global controller.
- Produces: current-song-only focus route and `Window(id: "mini-player")`.

- [ ] **Step 1: Add failing layout-policy tests**

```swift
func testFocusedPageUsesOnlyIdentityLyricsAndTransportSections() {
    XCTAssertEqual(
        PlaybackViewPolicy.focusedPageSections,
        [.identity, .lyrics, .transport]
    )
}

func testUnavailableLyricsUsesExplicitChineseMessage() {
    XCTAssertEqual(LyricsState.unavailable.displayText, "暂无歌词")
}
```

Define the exact section list without a queue case, so the compiler makes reintroducing a focused-page queue an explicit design change:

```swift
enum FocusedPlaybackSection: Equatable {
    case identity, lyrics, transport
}

extension PlaybackViewPolicy {
    static let focusedPageSections: [FocusedPlaybackSection] = [.identity, .lyrics, .transport]
}
```

- [ ] **Step 2: Run and confirm red**

Run: `swift test --package-path clients/apple --filter PlaybackViewPolicyTests`

Expected: failure until exact section order and `LyricsState` exist.

- [ ] **Step 3: Implement the focused page**

Create:

```swift
enum LyricsState: Equatable {
    case unavailable
    case loading
    case unsynced(String)
    case synced(lines: [String], currentLine: Int)
    case failed(String)

    var displayText: String {
        switch self {
        case .unavailable: "暂无歌词"
        case .loading: "正在加载歌词…"
        case .unsynced(let text): text
        case .synced(let lines, let current): lines.indices.contains(current) ? lines[current] : ""
        case .failed(let message): message
        }
    }
}
```

`NowPlayingView` receives `playback` and `close`. Its body uses a two-column `GeometryReader`: cover/identity at the left and `LyricsState.unavailable` at the center/right. The only bottom controls are previous, play/pause, next, timeline, shuffle, repeat, volume, and mute. Do not import, instantiate, or reference `QueuePanel`.

- [ ] **Step 4: Add an in-window focus route**

Use `@State private var isNowPlayingPresented = false` in `LibraryView`. When true, replace the library detail surface with `NowPlayingView`; keep the root safe-area player bar bound to the same controller. The back control sets the state false and does not call pause/stop.

- [ ] **Step 5: Add the mini player scene**

In `App.swift`:

```swift
Window("迷你播放器", id: "mini-player") {
    MiniPlayerView(playback: playback)
        .frame(width: 360, height: 132)
}
.windowResizability(.contentSize)
```

Add an `openWindow(id: "mini-player")` action to the main player bar menu. `MiniPlayerView` shows current cover/title/artist, previous/play-next, and a compact timeline; it never constructs an engine or controller.

- [ ] **Step 6: Verify focus and mini-player behavior**

Run: `swift test --package-path clients/apple --filter PlaybackViewPolicyTests`

Expected: exact focus sections pass and no queue section exists.

Run: `swift build --package-path clients/apple`

Expected: both window scenes compile.

Run the app with a local server: `./scripts/run-mac-client.sh --with-server`

Expected manual evidence: opening/closing focused and mini views leaves the same track playing; focused view contains no “接下来播放” text or queue list.

- [ ] **Step 7: Commit focus and mini-player UI**

```bash
git add clients/apple/Sources/Yevune/App.swift \
  clients/apple/Sources/Yevune/Views/LibraryView.swift \
  clients/apple/Sources/Yevune/Views/Playback/NowPlayingView.swift \
  clients/apple/Sources/Yevune/Views/Playback/MiniPlayerView.swift \
  clients/apple/Tests/YevuneTests/PlaybackViewPolicyTests.swift
git commit -m "feat(mac): 新增专注播放页与迷你播放器"
```

---

### Task 7: System media commands, logout cleanup, and final verification

**Files:**
- Create: `clients/apple/Sources/Yevune/Audio/SystemMediaCoordinator.swift`
- Create: `clients/apple/Sources/Yevune/Audio/PlaybackArtworkLoader.swift`
- Create: `clients/apple/Tests/YevuneTests/SystemMediaCoordinatorTests.swift`
- Modify: `clients/apple/Sources/Yevune/Audio/PlaybackController.swift`
- Modify: `clients/apple/Sources/Yevune/Model/LoginViewModel.swift`
- Modify: `clients/apple/Sources/Yevune/Views/LibraryView.swift`
- Modify: `clients/apple/Sources/Yevune/App.swift`
- Modify: `clients/apple/Tests/YevuneTests/LoginViewModelTests.swift`
- Modify: `clients/apple/Tests/YevuneTests/PlaybackControllerTests.swift`

**Interfaces:**
- Consumes: global controller state and intents.
- Produces: `SystemMediaCoordinating`, MediaPlayer production adapter, explicit logout cleanup, and final DoD evidence.

- [ ] **Step 1: Write failing system-media and logout tests**

Place `SystemMediaCoordinatorTests`, `FakeRemoteCommandSurface`, and `FakeNowPlayingSurface` in the new system-media test file. Insert `testLogoutClearsSessionAndPassword` inside the existing `LoginViewModelTests` class, where its private `FakeMusicClient` is visible. Insert `testControllerLoadsArtworkForCurrentSystemMetadata`, `FakeArtworkLoader`, and `RecordingSystemMediaCoordinator` in `PlaybackControllerTests.swift`.

```swift
import AppKit
import MediaPlayer
import XCTest
@testable import Yevune

@MainActor
final class SystemMediaCoordinatorTests: XCTestCase {
    func testRemoteCommandsInvokeSingleControllerHandlers() {
        let commands = FakeRemoteCommandSurface()
        let nowPlaying = FakeNowPlayingSurface()
        let coordinator = SystemMediaCoordinator(commands: commands, nowPlaying: nowPlaying)
        var calls: [String] = []

        coordinator.register(.init(
            play: { calls.append("play") }, pause: { calls.append("pause") },
            previous: { calls.append("previous") }, next: { calls.append("next") },
            seek: { calls.append("seek:\($0)") }
        ))
        commands.sendPlay(); commands.sendPrevious(); commands.sendSeek(42)

        XCTAssertEqual(calls, ["play", "previous", "seek:42.0"])
    }

    func testUpdatePublishesTrackTimingAndRate() {
        let nowPlaying = FakeNowPlayingSurface()
        let coordinator = SystemMediaCoordinator(
            commands: FakeRemoteCommandSurface(), nowPlaying: nowPlaying
        )

        coordinator.update(
            track: playbackTrack("1", title: "Song", duration: 180),
            elapsed: 42, duration: 180, state: .playing, artwork: nil
        )

        XCTAssertEqual(nowPlaying.info?[MPMediaItemPropertyTitle] as? String, "Song")
        XCTAssertEqual(nowPlaying.info?[MPNowPlayingInfoPropertyElapsedPlaybackTime] as? Double, 42)
        XCTAssertEqual(nowPlaying.info?[MPNowPlayingInfoPropertyPlaybackRate] as? Double, 1)
    }

    func testClearRemovesTargetsAndNowPlayingInfo() {
        let commands = FakeRemoteCommandSurface()
        let nowPlaying = FakeNowPlayingSurface()
        let coordinator = SystemMediaCoordinator(commands: commands, nowPlaying: nowPlaying)
        coordinator.register(.init(
            play: {}, pause: {}, previous: {}, next: {}, seek: { _ in }
        ))
        nowPlaying.info = ["title": "Song"]

        coordinator.clear()

        XCTAssertEqual(commands.targetCount, 0)
        XCTAssertNil(nowPlaying.info)
    }
}

func testLogoutClearsSessionAndPassword() async {
    let model = LoginViewModel(client: FakeMusicClient())
    model.server = "http://localhost"
    model.user = "u"
    model.password = "secret"
    await model.submit()
    XCTAssertNotNil(model.session)

    model.logout()

    XCTAssertNil(model.session)
    XCTAssertEqual(model.password, "")
}

func testControllerLoadsArtworkForCurrentSystemMetadata() async {
    let artwork = NSImage(size: NSSize(width: 10, height: 10))
    let loader = FakeArtworkLoader(image: artwork)
    let system = RecordingSystemMediaCoordinator()
    let controller = PlaybackController(
        resolver: RecordingMediaResolver(), engine: RecordingPlaybackEngine(),
        systemMedia: system, artworkLoader: loader
    )

    await controller.play(tracks: [playbackTrack("1")], startingAt: 0)
    await controller.waitForPendingArtworkForTesting()

    XCTAssertEqual(loader.loadedURLs.map(\.lastPathComponent), ["cover-1"])
    XCTAssertTrue(system.lastArtwork === artwork)
}

@MainActor
private final class FakeRemoteCommandSurface: RemoteCommandSurface {
    var handlers: RemotePlaybackHandlers?
    var targetCount: Int { handlers == nil ? 0 : 5 }
    func install(_ handlers: RemotePlaybackHandlers) { self.handlers = handlers }
    func removeAll() { handlers = nil }
    func sendPlay() { handlers?.play() }
    func sendPrevious() { handlers?.previous() }
    func sendSeek(_ seconds: TimeInterval) { handlers?.seek(seconds) }
}

@MainActor
private final class FakeNowPlayingSurface: NowPlayingSurface {
    var info: [String: Any]?
}

@MainActor
private final class FakeArtworkLoader: PlaybackArtworkLoading {
    let image: NSImage?
    private(set) var loadedURLs: [URL] = []
    init(image: NSImage?) { self.image = image }
    func load(url: URL) async -> NSImage? {
        loadedURLs.append(url)
        return image
    }
}

@MainActor
private final class RecordingSystemMediaCoordinator: SystemMediaCoordinating {
    private(set) var lastArtwork: NSImage?
    func register(_ handlers: RemotePlaybackHandlers) {}
    func update(
        track: Track?, elapsed: TimeInterval, duration: TimeInterval,
        state: PlaybackEngineState, artwork: NSImage?
    ) { lastArtwork = artwork }
    func clear() { lastArtwork = nil }
}
```

- [ ] **Step 2: Run and confirm red**

Run: `swift test --package-path clients/apple --filter SystemMediaCoordinatorTests`

Expected: compile failure because system media abstractions do not exist.

Run: `swift test --package-path clients/apple --filter LoginViewModelTests`

Expected: failure because logout is not implemented.

- [ ] **Step 3: Implement testable system media integration**

Define:

```swift
struct RemotePlaybackHandlers {
    let play: () -> Void
    let pause: () -> Void
    let previous: () -> Void
    let next: () -> Void
    let seek: (TimeInterval) -> Void
}

@MainActor
protocol RemoteCommandSurface: AnyObject {
    func install(_ handlers: RemotePlaybackHandlers)
    func removeAll()
}

@MainActor
protocol NowPlayingSurface: AnyObject {
    var info: [String: Any]? { get set }
}

@MainActor
protocol SystemMediaCoordinating: AnyObject {
    func register(_ handlers: RemotePlaybackHandlers)
    func update(track: Track?, elapsed: TimeInterval, duration: TimeInterval,
                state: PlaybackEngineState, artwork: NSImage?)
    func clear()
}
```

Production `SystemMediaCoordinator` wraps `MPRemoteCommandCenter.shared()` and `MPNowPlayingInfoCenter.default()`. Register targets once, map play/pause/previous/next/changePlaybackPosition, set title/artist/album/duration/elapsed/rate, and wrap artwork with `MPMediaItemArtwork`. `clear()` removes every target and sets `nowPlayingInfo = nil`.

Load system artwork outside the coordinator through an injectable async boundary:

```swift
@MainActor
protocol PlaybackArtworkLoading: AnyObject {
    func load(url: URL) async -> NSImage?
}

@MainActor
final class URLSessionPlaybackArtworkLoader: PlaybackArtworkLoading {
    func load(url: URL) async -> NSImage? {
        guard let (data, response) = try? await URLSession.shared.data(from: url),
              (response as? HTTPURLResponse)?.statusCode == 200 else { return nil }
        return NSImage(data: data)
    }
}
```

Do not print the cover URL on failure because it contains authentication query parameters.

- [ ] **Step 4: Connect controller and logout**

Inject `SystemMediaCoordinating` and `PlaybackArtworkLoading` into `PlaybackController`, with no-op defaults for tests that do not care about system state. Register handlers once in init. After resolving the current media, cancel the previous artwork task, load `coverURL`, verify the queue instance is still current, and update system metadata with that `NSImage`. Update elapsed/rate without reloading artwork. `shutdown()` must cancel the artwork task, call engine stop, clear queue and published media state, cancel pending transitions, clear retry state, and call `systemMedia.clear()`.

Add:

```swift
func logout() {
    password = ""
    session = nil
    errorMessage = nil
}
```

Expose logout from a user menu in `LibraryView`; the App-level closure calls `playback.shutdown()` before `login.logout()`.

- [ ] **Step 5: Run all Swift verification**

Run: `swift test --package-path clients/apple`

Expected: all Swift tests pass.

Run: `swift build --package-path clients/apple`

Expected: build succeeds.

Run: `for i in {1..20}; do swift test --package-path clients/apple --filter PlaybackControllerTests || exit 1; done`

Expected: controller recovery/system updates are deterministic for all 20 runs.

- [ ] **Step 6: Run repository-wide gates**

```bash
cargo test --manifest-path contract/Cargo.toml
cargo test --manifest-path server/Cargo.toml
cargo test --manifest-path core/Cargo.toml
cargo clippy --manifest-path contract/Cargo.toml -- -D warnings
cargo clippy --manifest-path server/Cargo.toml -- -D warnings
cargo clippy --manifest-path core/Cargo.toml -- -D warnings
cargo fmt --manifest-path contract/Cargo.toml --check
cargo fmt --manifest-path server/Cargo.toml --check
cargo fmt --manifest-path core/Cargo.toml --check
./scripts/tests/run-mac-client-test.sh
git diff --check
```

Expected: every command exits 0; clippy prints no warnings; launcher script prints `run-mac-client tests: PASS`.

- [ ] **Step 7: Perform real macOS playback smoke testing**

Run: `./scripts/run-mac-client.sh --with-server`

Verify and record evidence for each item:

1. Start from the middle of an album and advance in disc/track order.
2. Start a playlist containing duplicate tracks and preserve instance order.
3. Switch among library, playlist, and admin pages without interrupting playback.
4. Use player bar seek, volume, mute, shuffle, repeat, and the separate queue panel.
5. Open the focused page and verify it has no “接下来播放” label, queue list, or queue overlay.
6. Open/close the mini player and confirm the same current time/track remains.
7. Use hardware/media keys and inspect macOS now-playing metadata.
8. Log out and confirm audio stops and system now-playing information disappears.

- [ ] **Step 8: Commit system integration and verification support**

```bash
git add clients/apple/Sources/Yevune/Audio/SystemMediaCoordinator.swift \
  clients/apple/Sources/Yevune/Audio/PlaybackArtworkLoader.swift \
  clients/apple/Sources/Yevune/Audio/PlaybackController.swift \
  clients/apple/Sources/Yevune/Model/LoginViewModel.swift \
  clients/apple/Sources/Yevune/Views/LibraryView.swift \
  clients/apple/Sources/Yevune/App.swift \
  clients/apple/Tests/YevuneTests/SystemMediaCoordinatorTests.swift \
  clients/apple/Tests/YevuneTests/LoginViewModelTests.swift \
  clients/apple/Tests/YevuneTests/PlaybackControllerTests.swift
git commit -m "feat(mac): 接入系统播放控制与会话清理"
```

---

## Final Review Gate

After Task 7, use `superpowers:requesting-code-review` for a whole-branch review against `docs/superpowers/specs/2026-07-15-mac-playback-shell-design.md`. Fix every Critical or Important issue with TDD and rerun the relevant focused and global gates. Then use `superpowers:verification-before-completion` before reporting M3 as complete.
