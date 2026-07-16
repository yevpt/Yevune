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

After the minimum policy implementation, the filtered run passed all 11 cases. Coverage includes:

- exact 620pt compact/wide boundary;
- member/admin management actions;
- omission of absent or blank metadata and loaded-track duration calculation;
- `1·03` multi-disc numbering;
- refreshed-selection intersection;
- distinct member/admin empty-state copy.

## Components

- `AlbumHeaderView`: 200pt/144pt authenticated artwork with cover revision identity, semantic styling, single-line record metadata, play for all users, and admin-only cover/edit/access construction.
- `AlbumTrackList`: native `List(selection:)`, disc sections, policy-driven grid rows, list selection, double-click/Return playback, Command-A loaded selection, visible accessible play buttons, member playlist access, and admin-only management construction.
- `BatchActionBar`: remains outside list ownership; playback stays enabled during work while state-changing actions are disabled; member and admin action sets are structurally separated.
- `BatchOperationResultView`: determinate progress, current item, stop, concrete failed/skipped rows, retry, done, and Reduce Motion handling.
- `PlaybackTrackActions`: retained the existing `track + playback` initializer and behavior, while adding the minimum closure initializer required by the network-independent album component.

## Verification

```text
swift test --package-path clients/apple --filter AlbumWorkbenchPolicyTests
11 tests, 0 failures

swift test --package-path clients/apple
270 tests, 0 failures

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

## ⚠️ Notes

- The existing `PlaybackTrackActions` API accepted `PlaybackController` directly. Task 7 requires closure-only album components, so a behavior-preserving closure initializer was added without changing its existing initializer or queue semantics.
- Visual components compile on macOS 14 but are intentionally not reachable until Task 8 integrates them into `MediaDetailView`.
