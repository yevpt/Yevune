# M3 whole-branch final review fixes

Base: `297c456`

## Review findings addressed

1. **AVPlayerItem initial failure observation**
   - Added an `.initial` + `.new` KVO observation for the current item's `status` to the existing observation store.
   - A per-item gate deduplicates status failure and `AVPlayerItemFailedToPlayToEndTime` notification failure.
   - Replacement/stop/deinit cancel the item observation; queued callbacks retain the existing active-observation gate.
2. **Authenticated artwork isolation and production wiring**
   - `PlaybackController` now publishes the same ephemeral-loader-decoded `NSImage` used for system metadata.
   - `PlayerBar`, `NowPlayingView`, and `MiniPlayerView` consume `DecodedArtworkView(image: playback.artwork)` and never receive authenticated cover URLs.
   - `AlbumGridView` and `MediaDetailView` now use `AuthenticatedArtworkView`, backed by the existing ephemeral, cache-free, cookie-free and credential-storage-free loader.
   - Added structural security regression tests for all five production call sites.
3. **Canonical queue order after manual edits**
   - Insert, append, move, remove and clear-upcoming establish the visible UUID-instance sequence as the new canonical order.
   - Turning shuffle off therefore cannot undo a manual edit. Tests cover duplicate track IDs and preservation of the exact current UUID.
4. **920 pt player bar**
   - The production `PlayerBar` consumes `PlaybackViewPolicy.playerBarLayout(forWidth:)` through `GeometryReader`.
   - Widths below 1040 pt use a compact layout: title/artist summary, full previous/play-next/timeline transport, visible buffering/error state, queue entry, and an overflow menu for shuffle/repeat/mute/mini player. The volume slider is intentionally omitted at 920 pt.
5. **Focused playback status**
   - `NowPlayingView` consumes `focusedStatus`, rendering a restrained progress indicator for buffering and a safe two-line error label.
   - The focused page still has only identity, lyrics, and transport sections and contains no queue/up-next surface.
6. **Minors**
   - Invalid/non-seekable duration now always maps the slider value to zero.
   - Removed the unused `MainWindowChrome` policy and its disconnected test.

## TDD evidence

### RED

- `swift test --package-path clients/apple --filter 'PlaybackEngineTests|PlaybackQueueTests|PlaybackViewPolicyTests'`
  - Exit 1 before production changes.
  - Compilation failed because `PlaybackViewPolicy.playerBarLayout` / `focusedStatus` and their cases did not exist. The same test batch already contained the new item-status and canonical-order behavior tests.
- `swift test --package-path clients/apple --filter PlaybackControllerTests.testControllerLoadsArtworkForCurrentSystemMetadata`
  - Exit 1 before controller/UI production changes.
  - Exact failure: `value of type 'PlaybackController' has no member 'artwork'` (three assertions: loaded, superseded, and shutdown paths).

### GREEN

- `swift test --package-path clients/apple --filter 'PlaybackEngineTests|PlaybackQueueTests|PlaybackViewPolicyTests'`
  - 38 tests, 0 failures.
- `swift test --package-path clients/apple --filter 'PlaybackControllerTests|PlaybackViewPolicyTests'`
  - 55 tests, 0 failures.
- `swift test --package-path clients/apple --filter PlaybackArtworkSecurityTests`
  - 2 tests, 0 failures.
- `swift test --package-path clients/apple --filter PlaybackEngineTests`
  - 14 tests, 0 failures, including initial failed status, duplicate signal suppression, stale item isolation, and observer cleanup.

## Final gates

- PlaybackController + PlaybackEngine + PlaybackQueue filtered suite: **20/20 rounds PASS**.
- Fresh `swift test --package-path clients/apple`: **158 tests, 0 failures** (including both structural artwork security tests).
- `swift build --package-path clients/apple`: **PASS**.
- `cargo test --manifest-path {contract,server,core}/Cargo.toml`: **PASS**.
- `cargo clippy --manifest-path {contract,server,core}/Cargo.toml -- -D warnings`: **PASS**, no warnings.
- `cargo fmt --manifest-path {contract,server,core}/Cargo.toml --check`: **PASS**.
- `./scripts/tests/run-mac-client-test.sh`: `run-mac-client tests: PASS`.
- `git diff --check`: **PASS**.

## Real smoke

The implementer did not execute real UI/audio smoke during the code-fix phase. No runtime claim is made here. The main agent has separately prepared a local Garage + Rust server and two short FLAC fixtures and will record final UI/audio evidence after this commit.

## Final re-review follow-up: compact volume control

The compact 920 pt player bar now keeps a visible speaker button labeled `调整音量`. It opens a 240 pt popover containing a 0...1 volume slider, live percentage, and an explicit mute/unmute button. This preserves width safety without reducing volume control to mute-only behavior. `compactPlayerBarAccessories` is tested and consumed directly by the production compact path for volume, queue, and overflow discovery.

### RED

- `swift test --package-path clients/apple --filter PlaybackViewPolicyTests.testCompactPlayerBarKeepsDiscoverableVolumeQueueAndOverflowAccessories`
  - Exit 1 before production changes.
  - Exact compile failure: `PlaybackViewPolicy` had no member `compactPlayerBarAccessories`, and `.volume`, `.queue`, `.overflow` had no contextual type.

### GREEN

- `swift test --package-path clients/apple --filter PlaybackViewPolicyTests`: **17 tests, 0 failures**.
- `swift test --package-path clients/apple --filter PlaybackControllerTests`: **39 tests, 0 failures**.
- A fresh full Swift test/build and diff check were run before the follow-up commit; see the final handoff for their exact totals.

## Real-smoke follow-up: stable repeat-off queue tail

Real two-track smoke exposed an indefinite buffering presentation after the final track ended with repeat off. The controller now models this boundary as an explicit natural-completion state: it retains the current track and duration, pauses the engine, seeks to zero, publishes paused/elapsed-zero system metadata, and ignores late state/time callbacks from the completed item. Play (including the main toggle and remote play path) leaves completion state, seeks to zero again, and starts playback. Manual-next boundaries, repeat-all/one, and failure recovery keep their existing paths.

### RED

- Real smoke: two 4-second FLAC tracks, repeat off; after track two ended, PlayerBar/focus remained `暂停（正在缓冲）` at `0:04 / 0:04`.
- `swift test --package-path clients/apple --filter PlaybackControllerTests.testNaturalEndAtQueueTailSettlesPausedAndReplaysCurrentTrackFromZero`
  - Exit 1 with 8 expected assertion failures before implementation: controller remained playing/buffering at elapsed 4, engine had no pause/seek calls, and the main toggle did not replay.

### GREEN

- Focused queue-tail test: **1 test, 0 failures**, including retained track/system metadata, paused state, elapsed zero, late buffering/time suppression, and replay seek/play.
- Full `PlaybackControllerTests`: **40 tests, 0 failures**.
- PlaybackController + PlaybackQueue + PlaybackEngine: **20/20 repeated rounds PASS**.
- Fresh full Swift suite: **160 tests, 0 failures**; `swift build` and `git diff --check`: **PASS**.
- The main agent will rebuild and repeat the same real smoke after this commit; no post-fix runtime success is claimed here.

## Final epoch hardening after queue-tail re-review

The first queue-tail fix still reused the completed media item and guarded only selected event cases. The final implementation establishes an actual media epoch boundary:

- Every installed engine callback captures its `loadGeneration` and rejects all event variants when that generation is no longer current.
- Queue-tail completion increments the generation, detaches `engine.onEvent`, cancels pending artwork, and calls `engine.stop()` so the old item and its observers are removed. Current queue identity and already-published display/system metadata remain visible in the paused zero-elapsed presentation.
- Play from completion schedules exactly one new resolve/load of the same current QueueEntry UUID with autoplay. Main and remote play share this path; duplicate clicks cannot duplicate the load.
- Shutdown or explicit new playback cancels/supersedes a pending replay through task cancellation, generation, queue UUID, and completion-state gates.

### RED

- `swift test --package-path clients/apple --filter PlaybackControllerTests.testNaturalEndAtQueueTailDetachesOldMediaAndReloadsCurrentEntryForReplay`
  - After correcting the test fixture compile setup, exit 1 with 5 expected behavior failures: no stop/detach, late failure triggered a second resolve, captured events changed state/error, and replay did not reload the current media.

### GREEN

- Focused epoch/replay test: **1 test, 0 failures**. It captures the old callback, delivers late state/time/failed/ended before and after replay, verifies no recovery or state pollution, verifies stop/detach, and verifies exactly one authenticated resolve/load/autoplay for main + remote duplicate play.
- Pending replay shutdown and explicit-new-play supersession tests: **PASS**.
- Full `PlaybackControllerTests`: **42 tests, 0 failures**.
- PlaybackController + PlaybackEngine + PlaybackQueue: **20/20 repeated rounds PASS**.
- Fresh full Swift suite: **162 tests, 0 failures**; `swift build` and `git diff --check`: **PASS**.
- Post-fix real runtime verification remains assigned to the main agent; this report makes no new smoke-success claim.
