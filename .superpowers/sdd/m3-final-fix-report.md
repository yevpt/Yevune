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
