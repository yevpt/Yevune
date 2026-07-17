# Task 7 Report — Album Workbench Policy and Native Components

## Scope

- Added the pure `AlbumWorkbenchPolicy` and policy tests.
- Added the closure-driven album header, native track list, batch action bar, and batch result view.
- Did not integrate the components into `MediaDetailView`; that remains Task 8.
- Added no dependencies and created no editor, management sheet, or network-client ownership in these views.

## RED

Command:

```text
swift test --package-path clients/apple --filter AlbumWorkbenchPolicyTests
```

Observed the expected compile failure before product code existed:

```text
error: cannot find 'AlbumWorkbenchPolicy' in scope
error: fatalError
```

The failure was caused by the missing policy, not by a fixture or assertion error.

## GREEN

After the minimum policy implementation and review fixes, the filtered run passed all 16 cases. Coverage includes:

- exact 620pt compact/wide boundary;
- member/admin management actions;
- omission of absent or blank metadata and loaded-track duration calculation;
- `1·03` multi-disc numbering;
- refreshed-selection intersection;
- distinct member/admin empty-state copy.

## Review Fix RED / GREEN

Review-fix base HEAD: `ca499b66e63fb1d05ddd4351fbdaa64620398769`.
The review-fix HEAD is the ordinary append commit containing this report; its exact content-addressed SHA is included in the task handoff after commit creation.

### Shared header/track alignment

- RED: `AlbumWorkbenchPolicy.gridMetrics(width:)` did not exist.
- GREEN: compact and wide tests calculate the track-title leading offset from shared play width, horizontal spacing, and track-number width. `AlbumHeaderView` and `AlbumTrackList` both consume those metrics, so the record metadata and track title share the same leading alignment without visual-only magic numbers.

### Stable selection reconciliation

- RED: the ID-level reconciliation API did not exist, exposing that selection reconciliation was coupled to the non-empty `List` branch.
- GREEN: tests cover initial stale selection and a refresh to zero IDs. A stable `Group` around both list and empty states now reconciles on first appearance and every track-ID change.

### Missing disc normalization

- RED: normalized disc, multi-disc detection, and grouping APIs did not exist.
- GREEN: missing disc numbers normalize to disc 1. Detection, sorted grouping, section labels, and `D·NN` numbering all consume this policy. Disc 1 plus nil produces one disc-1 group; nil plus disc 2 produces disc groups 1 and 2.

## Components

- `AlbumHeaderView`: 200pt/144pt authenticated artwork with cover revision identity, semantic styling, single-line record metadata, play for all users, and admin-only cover/edit/access construction.
- `AlbumTrackList`: native `List(selection:)`, disc sections, policy-driven grid rows, list selection, double-click/Return playback, Command-A loaded selection, visible accessible play buttons, member playlist access, and admin-only management construction.
- `BatchActionBar`: remains outside list ownership; playback stays enabled during work while state-changing actions are disabled; member and admin action sets are structurally separated.
- `BatchOperationResultView`: determinate progress, current item, stop, concrete failed/skipped rows, retry, done, and Reduce Motion handling.
- `PlaybackTrackActions`: retained the existing `track + playback` initializer and behavior, while adding the minimum closure initializer required by the network-independent album component.

## Verification

```text
swift test --package-path clients/apple --filter AlbumWorkbenchPolicyTests
16 tests, 0 failures

swift test --package-path clients/apple
275 tests, 0 failures

swift build --package-path clients/apple
Build complete

git diff --check
clean
```

## Security and Accessibility Self-review

- Album component sources contain no `MusicClient`, client property, file importer, confirmation dialog, editor sheet, or management sheet.
- Member rendering never constructs edit, replace-cover, move, delete, or access-control buttons/menus; policy returns `[]` for members.
- Playback/queue and playlist actions remain available to members.
- Track play controls expose `播放 <title>` labels; Return and Command-A are supported; artwork and metadata have semantic labels.
- Result progress animation is removed when Reduce Motion is enabled.
- Header metadata and track rows consume one calculated grid metric source in compact and wide layouts.
- Selection reconciliation is attached to a stable wrapper, including initial appearance and transitions to the empty state.
- Missing disc numbers cannot create an unlabeled extra group because they normalize to disc 1 before detection, grouping, and numbering.

## ⚠️ Notes

- The existing `PlaybackTrackActions` API accepted `PlaybackController` directly. Task 7 requires closure-only album components, so a behavior-preserving closure initializer was added without changing its existing initializer or queue semantics.
- Visual components compile on macOS 14 but are intentionally not reachable until Task 8 integrates them into `MediaDetailView`.
