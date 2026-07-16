# Mac Album Workbench Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a modern native macOS album workbench that keeps playback primary while giving administrators deterministic loading, permission-safe metadata editing, explicit tag clearing, bounded batch operations, and reliable cover/move/delete workflows.

**Architecture:** Extend the shared Rust tag contract first, encode it in core, and implement explicit-null override semantics in the server without changing standard OpenSubsonic endpoints. In Swift, separate deterministic album loading, tag validation, batch coordination, pure presentation policy, and small SwiftUI components; the existing global `PlaybackController` remains the only playback state owner.

**Tech Stack:** Rust 2021, serde, sqlx/SQLite, axum, UniFFI 0.31, Swift 5.9, SwiftUI on macOS 14, XCTest, and the existing AVFoundation playback shell.

## Global Constraints

- Follow `AGENTS.md`; Rust server, SQLite, Garage, native SwiftUI, OpenSubsonic compatibility, and `/rest/ext/*` namespace boundaries are immutable.
- Use TDD for every product change: focused failing test, observed failure, minimum implementation, focused green run, then commit.
- Do not add dependencies, services, batch server endpoints, audio-tag write-back, lyrics, offline downloads, import-task-center work, or “接下来播放”.
- Keep write loops sequential and bounded; never load audio files into memory.
- A non-admin session must not construct tag, cover, move, delete, access-management, file-importer, or management-sheet UI.
- Do not modify global playback queue/engine behavior; playback must continue throughout album management.
- Support macOS 14 and a 920pt main window; use semantic colors, dynamic type, keyboard focus, VoiceOver, and Reduce Motion.
- Never log or persist authenticated cover or stream URLs.
- Commits must comply with `.agents/skills/git-commit/SKILL.md`; never use `--no-verify`.

## File Map

- `contract/src/media.rs`, `contract/src/lib.rs`: shared `TagField`.
- `core/src/ffi_types.rs`, `core/src/api/manage.rs`: UniFFI type and validated repeated `clear` encoding.
- `server/src/api/ext/library.rs`, `server/src/index/repo_media.rs`, `server/src/index/access.rs`: nullable patch parsing and effective-tag semantics on media and authorization reads.
- `server/src/api/system.rs`: `libraryManagement` version 2 advertisement while retaining version 1.
- `clients/apple/Sources/Yevune/Model/MediaViewModel.swift`: deterministic detail and cover state.
- `clients/apple/Sources/Yevune/Model/LibraryOperationErrorPresentation.swift`: consistent authentication/authorization and ordinary operation copy.
- `clients/apple/Sources/Yevune/Model/TagEditing.swift`: pure single/batch edit intent and validation.
- `clients/apple/Sources/Yevune/Model/TrackBatchOperationController.swift`: sequential progress, stop, result, retry.
- `clients/apple/Sources/Yevune/Views/Album/*`: policy, header, track list, batch bar, and result UI.
- `TagEditorView.swift`, `BatchTagEditorView.swift`, `MoveTrackView.swift`: independent management sheets.
- `MediaDetailView.swift`, `LibraryBrowserView.swift`, `LibraryView.swift`: composition, admin guard, access routing.
- Focused Rust/Swift test files named in each task; final evidence in `.superpowers/sdd/m5-album-workbench-report.md`.

No migration is required: `server/migrations/0001_init.sql` already declares `tag_overrides.value TEXT` nullable.

---

### Task 1: Shared Tag Clear Contract and Core Encoding

**Files:**
- Modify: `contract/src/media.rs:3-103`
- Modify: `contract/src/lib.rs:12-18`
- Modify: `core/src/ffi_types.rs:3-20`
- Modify: `core/src/api/manage.rs:6-83`
- Modify: `core/tests/manage_test.rs:1-62`

**Interfaces:**
- Produces `contract::TagField { Album, Artist, Genre, Year, Track, DiscNumber }`.
- Adds `TagUpdate.clear_fields: Vec<TagField>` and repeated `clear=<camelCase>` parameters.
- Preserves every existing set-only request.

- [ ] **Step 1: Write failing tests**

Append to `contract/src/media.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::TagField;

    #[test]
    fn tag_field_uses_stable_camel_case_names() {
        assert_eq!(serde_json::to_string(&TagField::Album).unwrap(), "\"album\"");
        assert_eq!(serde_json::to_string(&TagField::DiscNumber).unwrap(), "\"discNumber\"");
    }
}
```

Extend `core/tests/manage_test.rs` to construct `clear_fields: vec![TagField::Genre, TagField::DiscNumber]` and assert:

```rust
assert!(update.contains("clear=genre"));
assert!(update.contains("clear=discNumber"));
```

Add unit tests under `core/src/api/manage.rs`:

```rust
#[test]
fn rejects_setting_and_clearing_the_same_field() {
    let update = TagUpdate {
        title: None, album: None, artist: None, genre: Some("Jazz".into()),
        year: None, track: None, disc_number: None,
        clear_fields: vec![TagField::Genre],
    };
    assert!(tag_parameters("tr-1".into(), update).is_err());
}

#[test]
fn rejects_duplicate_clear_fields() {
    let update = TagUpdate {
        title: None, album: None, artist: None, genre: None,
        year: None, track: None, disc_number: None,
        clear_fields: vec![TagField::Year, TagField::Year],
    };
    assert!(tag_parameters("tr-1".into(), update).is_err());
}
```

- [ ] **Step 2: Observe RED**

Run:

```bash
cargo test --manifest-path contract/Cargo.toml tag_field_uses_stable_camel_case_names
cargo test --manifest-path core/Cargo.toml rejects_setting_and_clearing
```

Expected: compilation fails because `TagField`, `clear_fields`, and `tag_parameters` do not exist.

- [ ] **Step 3: Implement the shared enum and encoder**

Add before `Track` in `contract/src/media.rs` and re-export from `contract/src/lib.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum TagField { Album, Artist, Genre, Year, Track, DiscNumber }

pub use media::{Album, Artist, Genre, TagField, Track};
```

Declare the same variants with `#[uniffi::remote(Enum)]` in `core/src/ffi_types.rs`. Add `pub clear_fields: Vec<TagField>` to `TagUpdate`. Extract request construction into:

```rust
fn tag_parameters(id: String, update: TagUpdate) -> Result<Vec<(String, String)>> {
    let mut parameters = vec![("id".to_owned(), id)];
    let mut set_fields = HashSet::new();
    for (field, name, value) in [
        (None, "title", update.title),
        (Some(TagField::Album), "album", update.album),
        (Some(TagField::Artist), "artist", update.artist),
        (Some(TagField::Genre), "genre", update.genre),
    ] {
        if let Some(value) = value {
            if let Some(field) = field { set_fields.insert(field); }
            parameters.push((name.to_owned(), value));
        }
    }
    for (field, name, value) in [
        (TagField::Year, "year", update.year),
        (TagField::Track, "track", update.track),
        (TagField::DiscNumber, "discNumber", update.disc_number),
    ] {
        if let Some(value) = value {
            set_fields.insert(field);
            parameters.push((name.to_owned(), value.to_string()));
        }
    }
    let mut cleared = HashSet::new();
    for field in update.clear_fields {
        if !cleared.insert(field) || set_fields.contains(&field) {
            return Err(CoreError::InvalidRequest {
                message: "同一标签字段不能同时设置和清空".to_owned(),
            });
        }
        parameters.push(("clear".to_owned(), tag_field_name(field).to_owned()));
    }
    if parameters.len() == 1 {
        return Err(CoreError::InvalidRequest {
            message: "至少需要修改一个标签字段".to_owned(),
        });
    }
    Ok(parameters)
}
```

`tag_field_name` maps the six variants to `album`, `artist`, `genre`, `year`, `track`, and `discNumber`. `update_tags` must call `tag_parameters(id, update)?` before the existing HTTP method.

Before encoding numeric values, reject year outside `1...9999` and track/disc outside `1...999` with `CoreError::InvalidRequest`. Add focused tests for `year = 0`, `year = 10_000`, `track = 0`, and `disc_number = 1_000`; none may reach HTTP.

- [ ] **Step 4: Verify GREEN**

```bash
cargo test --manifest-path contract/Cargo.toml
cargo test --manifest-path core/Cargo.toml
cargo clippy --manifest-path contract/Cargo.toml -- -D warnings
cargo clippy --manifest-path core/Cargo.toml -- -D warnings
cargo fmt --manifest-path contract/Cargo.toml --check
cargo fmt --manifest-path core/Cargo.toml --check
```

Expected: all commands exit 0; captured URL contains both clear parameters.

- [ ] **Step 5: Commit**

```bash
git add contract/src/media.rs contract/src/lib.rs core/src/ffi_types.rs core/src/api/manage.rs core/tests/manage_test.rs
git commit -m "feat(core): 支持显式清空标签字段"
```

---

### Task 2: Server Explicit-NULL Override Semantics

**Files:**
- Modify: `server/src/api/ext/library.rs:17-292`
- Modify: `server/src/index/repo_media.rs:5-910`
- Modify: `server/src/index/access.rs`
- Modify: `server/src/api/system.rs:44-55`
- Modify: `server/tests/ext_test.rs:535-610,1162-1188,1360-1536`

**Interfaces:**
- Consumes Task 1 wire names.
- Produces repeated-clear parsing and `set_tag_overrides(id, &[(&str, Option<&str>)])`.
- Defines absent row = source, non-NULL row = set, NULL row = explicitly clear.

- [ ] **Step 1: Write failing integration tests**

Add to `server/tests/ext_test.rs`:

```rust
#[tokio::test]
async fn update_tags_can_explicitly_clear_optional_fields_without_source_fallback() {
    let fixture = Fixture::new().await;
    let uploaded = json(fixture.upload("library/clear/a.flac", &flac("a.flac")).await).await;
    let id = payload(&uploaded, "track")["id"].as_str().unwrap();
    let cleared = json(fixture.get(
        "admin",
        &format!("/rest/ext/updateTags?id={id}&clear=album&clear=artist&clear=genre&clear=year&clear=track&clear=discNumber"),
    ).await).await;
    assert_eq!(cleared["subsonic-response"]["status"], "ok", "{cleared}");
    let shown = json(fixture.get("admin", &format!("/rest/getSong?id={id}")).await).await;
    let song = payload(&shown, "song");
    for field in ["album", "artist", "genre", "year", "track", "discNumber"] {
        assert!(song.get(field).is_none(), "{field}: {shown}");
    }
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tag_overrides WHERE track_id = ? AND value IS NULL",
    ).bind(id.trim_start_matches("tr-").parse::<i64>().unwrap())
     .fetch_one(fixture.index.pool()).await.unwrap();
    assert_eq!(count, 6);
}
```

Add invalid cases `clear=title`, `clear=missing`, `genre=Jazz&clear=genre`, and duplicated `clear=year`; each must return OpenSubsonic error code 10. Extend discovery to assert `libraryManagement` versions `[1, 2]`. Add focused cases proving cleared genre disappears from `getGenres`, cleared numeric fields remain missing in album/search output, and deleting a NULL override row restores the scanned source value.

- [ ] **Step 2: Observe RED**

```bash
cargo test --manifest-path server/Cargo.toml update_tags_can_explicitly_clear -- --nocapture
cargo test --manifest-path server/Cargo.toml extensions_discovery_declares -- --nocapture
```

Expected: clear is rejected or falls back to source; advertised version is still `[1]`.

- [ ] **Step 3: Parse clear and build nullable patches**

Because axum `Query` does not reliably preserve repeated values, parse the existing `OriginalUri` with the already-declared `form_urlencoded` dependency:

```rust
fn parse_clear_fields(uri: &axum::http::Uri) -> Result<Vec<&'static str>, ()> {
    let mut fields = Vec::new();
    for (name, value) in form_urlencoded::parse(uri.query().unwrap_or_default().as_bytes()) {
        if name != "clear" { continue; }
        let field = match value.as_ref() {
            "album" => "album", "artist" => "artist", "genre" => "genre",
            "year" => "year", "track" => "track", "discNumber" => "discNumber",
            _ => return Err(()),
        };
        if fields.contains(&field) { return Err(()); }
        fields.push(field);
    }
    Ok(fields)
}
```

Convert `tag_values(params)` to `Some(value)`, append `(field, None)` for clear fields, reject conflicts/empty patches, then call the repository. Malformed patches return `response::parameter_error(format, "Tag patch is malformed")`.

- [ ] **Step 4: Implement NULL-aware repository reads and writes**

Change the repository signature and bind the optional value:

```rust
pub async fn set_tag_overrides(
    &self,
    id: i64,
    values: &[(&str, Option<&str>)],
) -> Result<bool> {
    let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tracks WHERE id = ?")
        .bind(id).fetch_one(self.pool).await?;
    if exists == 0 { return Ok(false); }
    let mut tx = self.pool.begin().await?;
    for (field, value) in values {
        sqlx::query(
            "INSERT INTO tag_overrides(track_id, field, value) VALUES(?, ?, ?) ON CONFLICT(track_id, field) DO UPDATE SET value = excluded.value",
        ).bind(id).bind(field).bind(value).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(true)
}
```

In `TRACK_SELECT`, join one alias per override field. For every optional field use:

```sql
CASE WHEN genre_override.track_id IS NULL THEN t.genre ELSE genre_override.value END AS genre
```

Title may use `COALESCE(title_override.value, t.title)` because title clearing is rejected. Update ordering, search, genre aggregation, FTS synchronization, and access-control SQL found by:

```bash
rg -n "COALESCE.*tag_overrides|LEFT JOIN tag_overrides|SELECT value FROM tag_overrides" server/src
```

For optional-number sorting only, wrap the final effective value in `COALESCE(..., 0)`; API values themselves must remain NULL. Genre aggregation must exclude final NULL values and never emit an empty genre. Apply the same existence-aware genre expression to `server/src/index/access.rs`, so a cleared genre cannot accidentally inherit the scanned genre during authorization.

- [ ] **Step 5: Advertise the revision and verify**

Change only the discovery value to:

```rust
{"name": "libraryManagement", "versions": [1, 2]}
```

Then run:

```bash
cargo test --manifest-path server/Cargo.toml
cargo clippy --manifest-path server/Cargo.toml -- -D warnings
cargo fmt --manifest-path server/Cargo.toml --check
```

Expected: all commands exit 0; old set-only tests and new clear tests both pass.

- [ ] **Step 6: Commit**

```bash
git add server/src/api/ext/library.rs server/src/index/repo_media.rs server/src/index/access.rs server/src/api/system.rs server/tests/ext_test.rs
git commit -m "feat(server): 支持显式清空标签覆盖"
```

---

### Task 3: Regenerate UniFFI and Restore Apple Build

**Files:**
- Regenerate: `clients/apple/Packages/YevuneCoreFFI/Sources/YevuneCoreFFI/*`
- Regenerate: `clients/apple/Packages/YevuneCoreFFI/YevuneCoreFFI.xcframework`
- Modify existing Swift `TagUpdate(...)` call sites under `clients/apple/Sources` and `clients/apple/Tests`.

**Interfaces:**
- Consumes Task 1 `TagField` and `TagUpdate.clearFields`.
- Produces Swift `TagField` and the additive `clearFields:` initializer argument.

- [ ] **Step 1: Observe stale-binding RED**

Add `clearFields: [.genre]` to one test construction, then run:

```bash
swift test --package-path clients/apple --filter TagEditorViewModelTests
```

Expected: compilation fails because the checked-in binding lacks the new API.

- [ ] **Step 2: Regenerate bindings**

```bash
clients/apple/Packages/YevuneCoreFFI/scripts/build-core.sh
```

Expected: Rust release build, UniFFI generation, and `xcodebuild -create-xcframework` exit 0.

- [ ] **Step 3: Update all existing Swift initializers**

Until real clear intents are added in Tasks 5–6, supply:

```swift
clearFields: []
```

Inspect every match from `rg -n "TagUpdate\(" clients/apple/Sources clients/apple/Tests`.

- [ ] **Step 4: Verify and commit**

```bash
swift test --package-path clients/apple --filter TagEditorViewModelTests
swift build --package-path clients/apple
git diff --check
git add clients/apple/Packages/YevuneCoreFFI clients/apple/Sources/Yevune/Model/TagEditorViewModel.swift clients/apple/Sources/Yevune/Views/BatchTagEditorView.swift clients/apple/Tests/YevuneTests/TagEditorViewModelTests.swift
git commit -m "build(apple): 更新标签清空绑定"
```

Expected: tests/build pass before the commit; the commit contains generated binding and initializer compatibility only.

---

### Task 4: Deterministic Album Detail and Cover State

**Files:**
- Create: `clients/apple/Tests/YevuneTests/MediaViewModelTests.swift`
- Create: `clients/apple/Sources/Yevune/Model/LibraryOperationErrorPresentation.swift`
- Modify: `clients/apple/Sources/Yevune/Model/MediaViewModel.swift:1-62`

**Interfaces:**
- Produces `AlbumDetailPhase`, `currentAlbumID`, `refreshError`, `coverError`, `coverRevision`, and generation-gated `load`, `refresh`, and `replaceCover`.
- Preserves `detail`, `coverURL`, `operationMessage`, and `makeTagEditor(for:)` during migration.

- [ ] **Step 1: Write failing continuation-controlled tests**

Create an actor fake that suspends each `getAlbum` call independently. Cover these cases:

```swift
@MainActor
final class MediaViewModelTests: XCTestCase {
    func testInitialLoadPublishesLoadingThenContent() async {
        let client = SuspendedAlbumClient()
        let model = MediaViewModel(client: client)
        let load = Task { await model.load(album: album("a")) }
        await client.waitForAlbumCalls(1)
        XCTAssertEqual(model.phase, .loading)
        await client.resolveAlbumCall(0, with: detail("a"))
        await load.value
        XCTAssertEqual(model.phase, .content)
        XCTAssertEqual(model.detail?.album.id, "a")
    }

    func testLateAlbumAResponseCannotOverwriteAlbumB() async {
        let client = SuspendedAlbumClient()
        let model = MediaViewModel(client: client)
        let a = Task { await model.load(album: album("a")) }
        await client.waitForAlbumCalls(1)
        let b = Task { await model.load(album: album("b")) }
        await client.waitForAlbumCalls(2)
        await client.resolveAlbumCall(1, with: detail("b"))
        await client.resolveAlbumCall(0, with: detail("a"))
        await a.value; await b.value
        XCTAssertEqual(model.currentAlbumID, "b")
        XCTAssertEqual(model.detail?.album.id, "b")
    }

    func testRefreshFailureRetainsContent() async {
        let client = SuspendedAlbumClient()
        let model = MediaViewModel(client: client)
        await resolveInitial(model, client: client, albumID: "a")
        let refresh = Task { await model.refresh(album: album("a"), successMessage: "完成") }
        await client.waitForAlbumCalls(2)
        await client.rejectAlbumCall(1)
        await refresh.value
        XCTAssertEqual(model.detail?.album.id, "a")
        XCTAssertNotNil(model.refreshError)
        XCTAssertEqual(model.phase, .content)
    }
}
```

Also prove: cover URL failure leaves detail content; replacement calls `setCoverArt`, reloads `AlbumDetail.album.coverArt`, resolves its URL, increments `coverRevision`, and publishes “封面已更新” only after success.
Add error-presentation tests: `CoreError.notAuthenticated` and server error code 50 produce “权限已变化，请重新登录”, while an ordinary network error retains its localized message without exposing authenticated URLs.

- [ ] **Step 2: Observe RED**

```bash
swift test --package-path clients/apple --filter MediaViewModelTests
```

Expected: compilation or behavior failure because the new state does not exist.

- [ ] **Step 3: Implement deterministic state**

Define:

```swift
enum AlbumDetailPhase: Equatable {
    case idle, loading, content, refreshing
    case failed(String)
}

@Published private(set) var currentAlbumID: String?
@Published private(set) var coverRevision = 0
@Published private(set) var phase: AlbumDetailPhase = .idle
@Published private(set) var refreshError: String?
@Published private(set) var coverError: String?
@Published private(set) var operationError: String?
private var generation = 0
private var detailTask: Task<AlbumDetail, Error>?
private var coverTask: Task<URL?, Error>?
```

`load(album:)` increments generation, cancels both tasks, clears content only when album ID changes, and sets `.loading` or `.refreshing`. Start detail and routed-cover requests together. Publish fetched detail and `.content` as soon as detail succeeds; do not wait for cover. If the fetched detail has a different cover ID than the routed snapshot, cancel/discard that cover result and resolve the fetched ID. Before publishing any result, require:

```swift
guard requestGeneration == generation, album.id == currentAlbumID else { return }
```

The final cover must correspond to `fetchedDetail.album.coverArt`. Cover failure sets `coverError` but leaves `.content`; refresh failure sets `refreshError` and retains old detail; initial failure sets `.failed(message)`.

Implement replacement without mutating authenticated URLs:

```swift
func replaceCover(album: Album, path: String) async {
    operationError = nil
    operationMessage = nil
    do {
        try await client.setCoverArt(albumID: album.id, localPath: path)
        await refresh(album: album, successMessage: "封面已更新")
        if refreshError == nil, coverError == nil, phase == .content { coverRevision += 1 }
    } catch {
        operationError = error.localizedDescription
    }
}
```

The view later forces the cache-free loader to rerun with `.id(coverRevision)`.

Create `LibraryOperationErrorPresentation.message(_:)`. Pattern-match generated `CoreError.notAuthenticated` and `.server(code: 50, ...)` to the permission-change copy above; use `error.localizedDescription` for other variants. Use this helper in MediaViewModel, TagEditorViewModel, MoveTrackViewModel, and TrackBatchOperationController instead of inventing different messages.

- [ ] **Step 4: Verify GREEN and commit**

```bash
swift test --package-path clients/apple --filter MediaViewModelTests
swift test --package-path clients/apple
swift build --package-path clients/apple
git add clients/apple/Sources/Yevune/Model/LibraryOperationErrorPresentation.swift clients/apple/Sources/Yevune/Model/MediaViewModel.swift clients/apple/Tests/YevuneTests/MediaViewModelTests.swift
git commit -m "feat(mac): 稳定专辑详情加载状态"
```

Expected: all commands pass before commit; late A never changes B state.

---

### Task 5: Validated Single and Batch Tag Drafts

**Files:**
- Create: `clients/apple/Sources/Yevune/Model/TagEditing.swift`
- Modify: `clients/apple/Sources/Yevune/Model/TagEditorViewModel.swift:1-77`
- Replace: `clients/apple/Tests/YevuneTests/TagEditorViewModelTests.swift`

**Interfaces:**
- Produces `TagDraft`, `TagDraftValidation`, `BatchFieldMode`, `BatchTagDraft`, and `TagUpdate` builders.
- Produces editor `isDirty`, `validation`, `canSave`, `isSubmitting`, and `save()`.
- Removes move/delete state and behavior from the tag editor.

- [ ] **Step 1: Write failing draft and submission tests**

```swift
func testUnchangedSingleDraftProducesNoUpdate() {
    XCTAssertNil(TagDraft(track: trackFixture()).makeUpdate())
}

func testClearingOptionalFieldsProducesClearFields() throws {
    var draft = TagDraft(track: trackFixture())
    draft.genre = ""
    draft.year = ""
    let update = try XCTUnwrap(draft.makeUpdate())
    XCTAssertEqual(update.clearFields, [.genre, .year])
    XCTAssertNil(update.genre)
    XCTAssertNil(update.year)
}

func testBlankTitleAndMalformedNumbersBlockSubmission() {
    var draft = TagDraft(track: trackFixture())
    draft.title = "  "
    draft.track = "abc"
    XCTAssertNotNil(draft.validation.title)
    XCTAssertNotNil(draft.validation.track)
    XCTAssertNil(draft.makeUpdate())
}

func testBatchDraftOnlyBuildsSafeCommonFields() throws {
    var draft = BatchTagDraft()
    draft.genre = .clear
    draft.year = .set("2025")
    let update = try XCTUnwrap(draft.makeUpdate())
    XCTAssertEqual(update.year, 2025)
    XCTAssertEqual(update.clearFields, [.genre])
}
```

Add async cases proving `save()` is single-flight, failure preserves draft and error, no-op performs no client call, and success sets `didSave`.

- [ ] **Step 2: Observe RED**

```bash
swift test --package-path clients/apple --filter TagEditorViewModelTests
```

Expected: compilation fails because draft types do not exist.

- [ ] **Step 3: Implement pure intent and validation**

Create:

```swift
enum BatchFieldMode: Equatable { case keep, set(String), clear }

struct TagDraftValidation: Equatable {
    var title: String?
    var year: String?
    var track: String?
    var discNumber: String?
    var isValid: Bool { [title, year, track, discNumber].allSatisfy { $0 == nil } }
}
```

`TagDraft` stores the original track and editable strings. Its builder trims whitespace, rejects blank title, parses year `1...9999`, track/disc `1...999`, and emits only changes. Clearing a previously non-nil optional field appends its `TagField`; an originally nil field left blank stays unchanged.

`BatchTagDraft` contains only album, artist, genre, and year modes. `.keep` emits nothing, `.set` validates and sets, `.clear` appends the field. It returns nil when all modes are keep. Do not expose title, track, or disc as common batch-set fields.

- [ ] **Step 4: Refactor TagEditorViewModel**

Use:

```swift
@Published var draft: TagDraft
@Published private(set) var didSave = false
@Published private(set) var isSubmitting = false
@Published private(set) var errorMessage: String?

var validation: TagDraftValidation { draft.validation }
var isDirty: Bool { draft.isDirty }
var canSave: Bool { isDirty && validation.isValid && !isSubmitting }

func save() async {
    guard canSave, let update = draft.makeUpdate() else { return }
    isSubmitting = true
    didSave = false
    errorMessage = nil
    defer { isSubmitting = false }
    do {
        try await client.updateTags(id: trackID, update: update)
        didSave = true
    } catch {
        errorMessage = error.localizedDescription
    }
}
```

Delete `moveKey`, `move`, `delete`, `didMove`, and `didDelete`.

- [ ] **Step 5: Verify GREEN and commit**

```bash
swift test --package-path clients/apple --filter TagEditorViewModelTests
swift test --package-path clients/apple
swift build --package-path clients/apple
git add clients/apple/Sources/Yevune/Model/TagEditing.swift clients/apple/Sources/Yevune/Model/TagEditorViewModel.swift clients/apple/Tests/YevuneTests/TagEditorViewModelTests.swift
git commit -m "feat(mac): 完善标签编辑语义"
```

Expected: all pass; invalid and no-op drafts make zero client calls.

---

### Task 6: Bounded Batch Operation Controller

**Files:**
- Create: `clients/apple/Sources/Yevune/Model/TrackBatchOperationController.swift`
- Create: `clients/apple/Tests/YevuneTests/TrackBatchOperationControllerTests.swift`
- Modify: `clients/apple/Sources/Yevune/Model/MediaViewModel.swift`

**Interfaces:**
- Consumes `TagUpdate` and validated `BatchTagDraft`.
- Produces `TrackBatchAction`, item states/results, progress, `run`, `stop`, `retryFailed`, and album-bound `reset`.
- Calls one injected refresh closure per completed or stopped run.

- [ ] **Step 1: Write failing sequence and stop tests**

```swift
func testUpdatesRunOneAtATimeAndRefreshExactlyOnce() async {
    let client = SuspendedBatchClient()
    let refresh = RefreshRecorder()
    let model = TrackBatchOperationController(client: client)
    let run = Task {
        await model.run(
            tracks: [track("1"), track("2")],
            action: .update(updateFixture()),
            onFinished: { await refresh.record() }
        )
    }
    await client.waitForCalls(1)
    XCTAssertEqual(await client.callCount(), 1)
    await client.resolveCall(0)
    await client.waitForCalls(2)
    await client.resolveCall(1)
    await run.value
    XCTAssertEqual(await refresh.count(), 1)
    XCTAssertEqual(model.completedCount, 2)
}

func testStopFinishesCurrentAndSkipsRemaining() async {
    let client = SuspendedBatchClient()
    let model = TrackBatchOperationController(client: client)
    let run = Task { await model.run(tracks: tracks(3), action: .delete, onFinished: {}) }
    await client.waitForCalls(1)
    model.stop()
    await client.resolveCall(0)
    await run.value
    XCTAssertEqual(await client.callCount(), 1)
    XCTAssertEqual(model.results.map(\.state), [.succeeded, .skipped, .skipped])
}
```

Add cases for partial failure continuing to track 3, retry sending only failed IDs, reentry refusal, and maximum in-flight calls equal to one.
Add a reset case proving that leaving album A clears A results only after its current request completes and that a late A completion cannot republish results into album B.

- [ ] **Step 2: Observe RED**

```bash
swift test --package-path clients/apple --filter TrackBatchOperationControllerTests
```

Expected: compilation fails because controller types do not exist.

- [ ] **Step 3: Implement the coordinator**

```swift
enum TrackBatchAction { case update(TagUpdate), delete }
enum TrackBatchItemState: Equatable {
    case pending, succeeded, skipped
    case failed(String)
}
struct TrackBatchItemResult: Identifiable, Equatable {
    let track: Track
    var id: String { track.id }
    var state: TrackBatchItemState
}
```

The `@MainActor` controller owns `albumID`, `results`, `currentTrackID`, `isRunning`, `stopRequested`, and a generation. `run` snapshots input order, refuses reentry, then uses a plain sequential `for` loop. Before each call, a stop request marks all remaining items skipped. Await one client write, record success/error on that item only when album ID and generation still match, continue after failure, then call `onFinished` exactly once for the originating album. `retryFailed` builds a stable list from failed results and uses the same private runner. `reset(for:)` stops an old run, increments generation, and clears old results. Do not use task groups, detached tasks, or parallel requests.

Add to MediaViewModel:

```swift
func refreshAfterBatch(album: Album, message: String) async {
    await refresh(album: album, successMessage: message)
}

func makeBatchController() -> TrackBatchOperationController {
    TrackBatchOperationController(client: client)
}
```

- [ ] **Step 4: Verify GREEN and commit**

```bash
swift test --package-path clients/apple --filter TrackBatchOperationControllerTests
swift test --package-path clients/apple --filter MediaViewModelTests
swift test --package-path clients/apple
swift build --package-path clients/apple
git add clients/apple/Sources/Yevune/Model/TrackBatchOperationController.swift clients/apple/Sources/Yevune/Model/MediaViewModel.swift clients/apple/Tests/YevuneTests/TrackBatchOperationControllerTests.swift clients/apple/Tests/YevuneTests/MediaViewModelTests.swift
git commit -m "feat(mac): 增加曲目批量操作进度"
```

Expected: all pass; observed in-flight write count never exceeds one.

---

### Task 7: Album Workbench Policy and Native Components

**Files:**
- Create: `clients/apple/Sources/Yevune/Views/Album/AlbumWorkbenchPolicy.swift`
- Create: `clients/apple/Sources/Yevune/Views/Album/AlbumHeaderView.swift`
- Create: `clients/apple/Sources/Yevune/Views/Album/AlbumTrackList.swift`
- Create: `clients/apple/Sources/Yevune/Views/Album/BatchActionBar.swift`
- Create: `clients/apple/Sources/Yevune/Views/Album/BatchOperationResultView.swift`
- Create: `clients/apple/Tests/YevuneTests/AlbumWorkbenchPolicyTests.swift`

**Interfaces:**
- Produces responsive column sets and admin-only actions.
- Components consume closures and never call a network client directly.
- Reuses `PlaybackTrackActions` and `PlaybackViewPolicy.albumPlaybackOrder`.

- [ ] **Step 1: Write failing pure-policy tests**

```swift
@MainActor
final class AlbumWorkbenchPolicyTests: XCTestCase {
    func testCompactInspectorUsesEssentialColumns() {
        XCTAssertEqual(
            AlbumWorkbenchPolicy.columns(width: 480),
            [.trackNumber, .titleAndArtist, .duration]
        )
    }

    func testWideDetailAddsArtistAndFormatColumns() {
        XCTAssertEqual(
            AlbumWorkbenchPolicy.columns(width: 720),
            [.trackNumber, .title, .artist, .duration, .format]
        )
    }

    func testMembersNeverReceiveManagementActions() {
        XCTAssertEqual(AlbumWorkbenchPolicy.managementActions(isAdmin: false), [])
        XCTAssertEqual(
            AlbumWorkbenchPolicy.managementActions(isAdmin: true),
            [.editTags, .replaceCover, .move, .delete, .manageAccess]
        )
    }
}
```

Also test metadata omission for missing year/genre, `1·03` multi-disc numbering, selection reconciliation after refresh, and distinct member/admin empty copy.

- [ ] **Step 2: Observe RED**

```bash
swift test --package-path clients/apple --filter AlbumWorkbenchPolicyTests
```

Expected: compilation fails because the policy does not exist.

- [ ] **Step 3: Implement pure policy**

Define `AlbumWorkbenchColumn` and `AlbumManagementAction` as `Equatable` enums and implement:

```swift
static func columns(width: CGFloat) -> [AlbumWorkbenchColumn]
static func managementActions(isAdmin: Bool) -> [AlbumManagementAction]
static func metadata(album: Album, tracks: [Track]) -> String
static func trackNumber(_ track: Track, isMultiDisc: Bool) -> String
static func reconciledSelection(_ selection: Set<String>, tracks: [Track]) -> Set<String>
static func emptyMessage(isAdmin: Bool) -> String
```

Use the exact 620pt detail-width threshold. Join only present metadata with ` · `; derive total duration from loaded tracks. Return no management actions for members.

- [ ] **Step 4: Implement closure-driven visual components**

`AlbumHeaderView` receives album/detail, cover URL/revision, admin state, and play/cover/album-access/artist-access/edit-album closures. Use `AuthenticatedArtworkView(...).id(coverRevision)`, 200pt artwork in wide space, 144pt when compact, semantic colors, and dynamic fonts. Its single-line “唱片标签” joins year, genre, song count, and duration with ` · ` and aligns to the track grid; do not add decorative badges, fixed hex colors, or unrelated motion.

`AlbumTrackList` uses `List(selection:)` with `Section` per disc for multi-disc albums. Each row is a `Grid` driven by policy columns. Single click selects; double click and Return call `onPlay`; Command-A replaces selection with all loaded track IDs. The visible play button labels “播放 <title>”. Always include `PlaybackTrackActions`; only construct edit/move/delete/track-access items under `if isAdmin`. Joining a playlist remains available to members.

`BatchActionBar` sits outside the scrolling list. Members receive “加入歌单”和“取消选择”; admins additionally receive “修改标签”和含删除的“更多”. Disable state-changing controls while a batch operation runs, but never disable playback.

`BatchOperationResultView` renders determinate progress, current track, “停止”, concrete failure/skipped rows, “重试失败项”, and “完成”. Respect Reduce Motion.

- [ ] **Step 5: Verify GREEN and commit**

```bash
swift test --package-path clients/apple --filter AlbumWorkbenchPolicyTests
swift build --package-path clients/apple
git add clients/apple/Sources/Yevune/Views/Album clients/apple/Tests/YevuneTests/AlbumWorkbenchPolicyTests.swift
git commit -m "feat(mac): 构建专辑整理台组件"
```

Expected: tests/build exit 0 with no API unavailable below macOS 14.

---

### Task 8: Editors, Move Workflow, and Root Integration

**Files:**
- Modify: `clients/apple/Sources/Yevune/Views/TagEditorView.swift:1-53`
- Modify: `clients/apple/Sources/Yevune/Views/BatchTagEditorView.swift:1-60`
- Create: `clients/apple/Sources/Yevune/Model/MoveTrackViewModel.swift`
- Create: `clients/apple/Sources/Yevune/Views/MoveTrackView.swift`
- Replace: `clients/apple/Sources/Yevune/Views/MediaDetailView.swift`
- Modify: `clients/apple/Sources/Yevune/Views/Library/LibraryBrowserView.swift:3-225`
- Modify: `clients/apple/Sources/Yevune/Views/LibraryView.swift:286-317`
- Create: `clients/apple/Tests/YevuneTests/MoveTrackViewModelTests.swift`
- Modify: `clients/apple/Tests/YevuneTests/PlaybackArtworkSecurityTests.swift` to scan `Views/Album/AlbumHeaderView.swift` after artwork moves out of `MediaDetailView.swift`.

**Interfaces:**
- Consumes Tasks 4–7.
- Produces `MediaDetailView(..., isAdmin:onManageAccess:)` and `LibraryBrowserView(..., onManageAccess:)`.
- Preserves global playback-controller identity and M4 compact navigation.

- [ ] **Step 1: Add failing integration-source assertions**

Extend policy/security tests with source checks:

```swift
XCTAssertTrue(mediaDetailSource.contains("if isAdmin"))
XCTAssertFalse(mediaDetailSource.contains("Button(\"替换封面\") { importing = true }"))
XCTAssertTrue(libraryViewSource.contains("onManageAccess: { accessTarget = $0 }"))
XCTAssertFalse(tagEditorSource.contains("移动曲目"))
XCTAssertFalse(tagEditorSource.contains("删除曲目"))
```

These supplement, not replace, pure policy tests.

Update the authenticated-library-artwork source list by replacing `Sources/Yevune/Views/MediaDetailView.swift` with `Sources/Yevune/Views/Album/AlbumHeaderView.swift`; keep the assertions that the consumer contains `AuthenticatedArtworkView` and never `AsyncImage`.

- [ ] **Step 2: Observe RED**

```bash
swift test --package-path clients/apple --filter AlbumWorkbenchPolicyTests
swift test --package-path clients/apple --filter PlaybackArtworkSecurityTests
```

Expected: assertions fail against the old mixed management form and unguarded cover action.

- [ ] **Step 3: Build single and batch editor sheets**

`TagEditorView` uses `NavigationStack` + grouped `Form`, inline field errors, and toolbar actions:

```swift
ToolbarItem(placement: .cancellationAction) {
    Button("取消") { requestDismiss() }
}
ToolbarItem(placement: .confirmationAction) {
    Button("保存更改") { Task { await model.save() } }
        .disabled(!model.canSave)
}
```

On success call `onSuccess("标签已更新")` and dismiss. A dirty cancellation requires “放弃更改” confirmation. Do not include move/delete controls.

`BatchTagEditorView` owns `BatchTagDraft`. Each album/artist/genre/year row has “保持 / 设置 / 清空”; “设置” reveals a field. Disable confirmation for no-op or invalid state. Return one validated `TagUpdate` through a closure; do not call `MediaViewModel` directly.

- [ ] **Step 4: Implement independent move UI**

Write `MoveTrackViewModelTests` first to prove prefill, the three local validation failures, single-flight submission, server-error preservation, and success. Run `swift test --package-path clients/apple --filter MoveTrackViewModelTests` and observe compilation failure.

`MoveTrackViewModel` pre-fills `track.path`, publishes `isSubmitting`, `didMove`, and inline errors, and validates:

```swift
var pathError: String? {
    let value = destination.trimmingCharacters(in: .whitespacesAndNewlines)
    if !value.hasPrefix("library/") { return "路径必须以 library/ 开头" }
    if value.contains("..") { return "路径不能包含 .." }
    if value == track.path { return "请输入不同的目标路径" }
    return nil
}
```

Prevent duplicate submission; invoke `client.moveTrack` once. Server remains the conflict/authorization boundary. `MoveTrackView` only binds the model, renders validation, and reports success; it does not own or call a client.

- [ ] **Step 5: Compose MediaDetailView**

Add `isAdmin`, `onImportMusic`, and optional access callback. The view owns only selection/sheet/dialog state plus a batch controller created through `MediaViewModel.makeBatchController()`.

Render model phases as follows:

- no-content `.idle/.loading`: stable header/list skeleton;
- no-content `.failed`: `ContentUnavailableView` with retry;
- `.content/.refreshing`: header, local banners, list, and conditional batch bar.

On album ID change, clear selection and album-bound sheets and call `batch.reset(for: album.id)`. On same-album refresh, reconcile IDs with the policy. Keep `.task(id: album.id) { await model.load(album: album) }`.

Create file importer and every management sheet only inside `if isAdmin`. The admin empty state calls the existing `onImportMusic`; the member empty state has no action. The header action “修改专辑信息” opens the safe batch editor for every track and explains that common fields apply to the whole album. Batch writes use the controller and one `refreshAfterBatch`; cover uses `replaceCover(album:path:)`; single tag/move/delete refresh the same album. Single delete confirmation names the track and states that deletion is irreversible. No path invokes playback pause/shutdown.

- [ ] **Step 6: Route admin and access state from root**

Add `let onManageAccess: (AccessScopeTarget) -> Void` to `LibraryBrowserView`, then pass:

```swift
MediaDetailView(
    album: album,
    model: media,
    playlists: playlists,
    playback: playback,
    isAdmin: session.admin,
    onImportMusic: onImportMusic,
    onManageAccess: session.admin ? onManageAccess : nil
)
```

At `LibraryView` construction pass:

```swift
onManageAccess: { accessTarget = $0 }
```

Reuse the existing access model and sheet; do not create another.

- [ ] **Step 7: Verify GREEN and commit**

```bash
swift test --package-path clients/apple --filter TagEditorViewModelTests
swift test --package-path clients/apple --filter TrackBatchOperationControllerTests
swift test --package-path clients/apple --filter MoveTrackViewModelTests
swift test --package-path clients/apple --filter AlbumWorkbenchPolicyTests
swift test --package-path clients/apple --filter PlaybackArtworkSecurityTests
swift test --package-path clients/apple
swift build --package-path clients/apple
git diff --check
git add clients/apple/Sources/Yevune/Views clients/apple/Sources/Yevune/Model clients/apple/Tests/YevuneTests
git commit -m "feat(mac): 集成专辑整理台"
```

Expected: all pass; member policy cannot construct any management surface.

---

### Task 9: Full Gates and Real macOS Acceptance

**Files:**
- Create: `.superpowers/sdd/m5-album-workbench-report.md`
- Defect rule: after adding a reproducing failing test, modify only that test and the smallest owning source file, then name both in the resulting fix commit.

**Interfaces:**
- Produces current-run proof for every M5 requirement and repository gate.

- [ ] **Step 1: Run all automated gates on final code**

```bash
swift test --package-path clients/apple
swift build --package-path clients/apple
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
test -z "$(git rev-list --merges HEAD)"
```

Expected: every command exits 0. Record current test counts and timestamps, not earlier runs.

- [ ] **Step 2: Prepare real acceptance state**

Use the repository launcher, local Garage, and Rust server. Seed at least two albums, including one multi-disc album with cover, genre, year, track, and disc metadata. Use one administrator and one non-admin family member. Record only non-secret URLs and entity counts; never record passwords, bearer tokens, or authenticated media URLs.

- [ ] **Step 3: Verify administrator behavior**

At 920pt and a width `>= 1180pt`, record evidence that:

1. header, artwork, metadata label, track list, batch bar, and global player do not clip/overlap;
2. play-all and double-click-from-middle use global playback, which survives every management flow;
3. rapid A/B/A navigation never shows the wrong detail or cover;
4. setting then clearing album, artist, genre, year, track, and disc remains consistent in detail, search, ordering, and genre filtering;
5. batch progress is sequential; partial failure continues, stop skips remaining, retry sends only failures;
6. cover replacement appears without leaving the page;
7. invalid move is blocked locally, valid move refreshes, and delete requires confirmation;
8. album/artist/track visibility actions open the existing access editor.

- [ ] **Step 4: Verify member and native interaction behavior**

As the member, verify no edit/cover/move/delete/access/import/management sheet exists, while playback, selection, and add-to-playlist work. Verify Shift/Command selection, Command-A, double-click playback, Return where supported, Escape dismissal, visible focus, VoiceOver labels, and uninterrupted player controls while admin batch results are visible.

- [ ] **Step 5: Write evidence report**

Create `.superpowers/sdd/m5-album-workbench-report.md` with final commit, current date/time, macOS/Xcode/Swift/Rust versions, every gate command/result/count, fixture counts, both roles, both widths, and a requirement-to-observation table. Do not leave unfinished markers or unverified claims.

- [ ] **Step 6: Fix defects with separate RED/GREEN commits**

For each observed defect, first add the smallest reproducing test, observe failure, patch the owning component, rerun focused and impacted suites, then commit with a compliant message such as:

```bash
git commit -m "fix(mac): 修正专辑整理台交互"
git commit -m "fix(server): 修正标签清空读取"
```

Do not weaken the specification or label a product defect as environmental.

- [ ] **Step 7: Commit verified evidence**

```bash
git add .superpowers/sdd/m5-album-workbench-report.md
git commit -m "test(mac): 记录专辑整理台验收"
git status --short --branch
```

Expected: clean worktree after report commit.

---

## Final Completion Audit

Before declaring M5 complete, map every criterion in `docs/superpowers/specs/2026-07-17-mac-album-workbench-design.md` to current evidence:

- playback-first hierarchy and continuity: component source, playback regression tests, real smoke;
- member/admin split: policy tests, construction guard, server authorization, both real roles;
- deterministic detail/cover state: continuation-controlled tests, rapid-navigation smoke;
- keep/set/clear: serde, core URL, server NULL, Swift draft, and real UI/API evidence;
- bounded progress/stop/retry: suspended-client tests and observed operation;
- cover/move/delete refresh: model tests and real observation;
- 920pt/wide layout, keyboard, VoiceOver, Reduce Motion: policy/build and real macOS evidence;
- repository gates: commands run on final HEAD and captured in the report.

If any evidence is missing, stale, indirect, or contradictory, continue implementation or verification and do not claim completion.
