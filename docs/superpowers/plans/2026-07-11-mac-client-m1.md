# Mac 管理客户端 M1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 交付可在 macOS 14+ 运行的原生管理客户端，完成登录、曲库管理、封面替换与试听闭环。

**Architecture:** `core` 是依赖 `contract` 的 Rust HTTP 客户端，并以 UniFFI 暴露唯一的 `MusicClient` 门面；`clients/apple` 仅持有 SwiftUI 视图、视图模型与 AVFoundation 试听。上传在 Rust 内从本地路径分块读取并发出 multipart，绝不跨 FFI 或在内存中聚合整个音频文件。

**Tech Stack:** Rust 2021、reqwest、uniffi、tokio、serde、serde_json、Swift 5.9/SwiftUI、XCTest、AVFoundation。

## Global Constraints

- 服务端仍为 Rust；SQLite 在本地磁盘、Garage 是音频与转码缓存的唯一对象存储。
- Core 可直接依赖 `reqwest`、`uniffi`、`tokio`、`serde` 与 `serde_json`；后 3 个复用仓库既有依赖，分别用于异步运行时与 JSON 协议解析。
- 所有 OpenSubsonic 端点保持兼容；新增服务端能力只能在 `/rest/ext/*` 并在扩展发现端点声明。
- UI 不实现 HTTP、认证或管理协议；`MusicClient` 与其可 mock 的 Swift 协议是唯一桥梁。
- 每个行为先测试红，再写最小实现并验证绿；切片末尾运行格式化、clippy 与对应端到端验证。
- 范围仅限 Mac M1；不实现管理员/访问控制界面、离线下载或 iOS target。

---

## File layout

| Path | Responsibility |
| --- | --- |
| `core/Cargo.toml` | Core crate、reqwest 与 UniFFI 构建配置。 |
| `core/src/{config,auth,http,error}.rs` | 连接、认证请求参数、JSON 信封解析与错误映射。 |
| `core/src/api/{browse,manage,scan,media}.rs` | 分切片的协议操作。 |
| `core/src/client.rs` | 可供 UniFFI 调用的 `MusicClient` 门面。 |
| `core/tests/*.rs` | 对本地 axum mock 的协议、URL 与流式上传集成测试。 |
| `clients/apple/Package.swift` | macOS App 与 XCTest targets。 |
| `clients/apple/Packages/CoreFFI` | 生成绑定的 binary target、生成脚本和 xcframework。 |
| `clients/apple/Sources/MusicApp/Model` | 仅调用可 mock `MusicClientProviding` 的视图模型。 |
| `clients/apple/Sources/MusicApp/Views` | 原生 SwiftUI 管理界面。 |
| `clients/apple/Sources/MusicApp/Audio/PreviewPlayer.swift` | 只消费 `stream_url` 的 AVPlayer 封装。 |
| `clients/apple/Tests/MusicAppTests` | 视图模型与试听器 XCTest。 |
| `server/src/api/ext/cover.rs` | `setCoverArt` 的流式图片上传与关联。 |
| `server/tests/ext_test.rs` | 新扩展的授权、流式与索引行为测试。 |

### Task 1: S0 — Rust core、UniFFI 和登录闭环

**Files:**
- Create: `core/Cargo.toml`, `core/build.rs`, `core/src/lib.rs`, `core/src/config.rs`, `core/src/auth.rs`, `core/src/http.rs`, `core/src/error.rs`, `core/src/client.rs`, `core/tests/login_test.rs`
- Create: `clients/apple/Package.swift`, `clients/apple/Sources/MusicApp/App.swift`, `clients/apple/Sources/MusicApp/Model/LoginViewModel.swift`, `clients/apple/Sources/MusicApp/Views/LoginView.swift`, `clients/apple/Tests/MusicAppTests/LoginViewModelTests.swift`, `clients/apple/Packages/CoreFFI/scripts/build-core.sh`
- Modify: `README.md`

**Interfaces:**
- Produces `MusicClient::new()`, `MusicClient::login(server, user, password) -> Result<Session, CoreError>`, and `MusicClient::ping() -> Result<(), CoreError>`.
- Produces Swift `MusicClientProviding.login(server:user:password:) async throws` and `LoginViewModel.submit()`.

- [ ] **Step 1: Write failing Rust login tests**

```rust
#[tokio::test]
async fn login_pings_with_subsonic_credentials() {
    let client = MusicClient::new();
    client.login(server.url(), "admin".into(), "secret".into()).await.unwrap();
    client.ping().await.unwrap();
}
```

- [ ] **Step 2: Verify red**

Run: `cargo test --manifest-path core/Cargo.toml --test login_test`

Expected: compile failure because `MusicClient` does not exist.

- [ ] **Step 3: Implement the minimal authenticated JSON client and UniFFI export**

```rust
pub async fn login(&self, server: String, user: String, password: String) -> Result<Session> {
    self.state.write().await.replace(Session::new(server, user, password));
    self.ping().await?;
    Ok(self.session().await?)
}
```

- [ ] **Step 4: Verify green and bindings**

Run: `cargo test --manifest-path core/Cargo.toml --test login_test && clients/apple/Packages/CoreFFI/scripts/build-core.sh && swift test --package-path clients/apple`

Expected: login mock sees `u`, `p`, `v=1.16.1`, `c=music-mac`, `f=json`; Swift target imports generated module.

- [ ] **Step 5: Commit**

```bash
git add core clients/apple README.md
git commit -m "feat(core): 打通 Mac 登录与 UniFFI 构建链路"
```

### Task 2: S1 — 浏览与搜索

**Files:**
- Create: `core/src/api/browse.rs`, `core/tests/browse_test.rs`
- Create: `clients/apple/Sources/MusicApp/Model/LibraryViewModel.swift`, `clients/apple/Sources/MusicApp/Views/LibraryView.swift`, `clients/apple/Sources/MusicApp/Views/AlbumDetailView.swift`, `clients/apple/Tests/MusicAppTests/LibraryViewModelTests.swift`
- Modify: `core/src/client.rs`, `clients/apple/Sources/MusicApp/App.swift`

**Interfaces:**
- Produces `list_artists()`, `list_albums(sort, offset, size)`, `get_artist(id)`, `get_album(id)`, `get_song(id)`, and `search(query)`.
- Produces `LibraryViewModel.load()`, `LibraryViewModel.search(query:)`, and `LibraryViewModel.select(album:)`.

- [ ] **Step 1: Write failing protocol and view-model tests**

```rust
#[tokio::test]
async fn search_decodes_artists_albums_and_songs_from_search3() { /* mock JSON envelope */ }
```

```swift
func testSearchPublishesAlbumsReturnedByCore() async throws {
    let model = LibraryViewModel(client: FakeClient(searchAlbums: [.fixture]))
    await model.search(query: "Blue")
    XCTAssertEqual(model.albums.map(\.name), ["Blue"])
}
```

- [ ] **Step 2: Verify red**

Run: `cargo test --manifest-path core/Cargo.toml --test browse_test && swift test --package-path clients/apple --filter LibraryViewModelTests`

Expected: missing browse APIs and view model.

- [ ] **Step 3: Implement minimal envelope DTOs, browse API and native list/detail UI**

```rust
pub async fn search(&self, query: String) -> Result<SearchResult> {
    self.browse.search3(&query).await
}
```

- [ ] **Step 4: Verify green and an authenticated real-server browse/search check**

Run: `cargo test --manifest-path core/Cargo.toml --test browse_test && swift test --package-path clients/apple --filter LibraryViewModelTests`

Expected: artist/album/song payloads render through the view model without Swift HTTP code.

- [ ] **Step 5: Commit**

```bash
git add core clients/apple
git commit -m "feat(mac): 浏览曲库并支持搜索"
```

### Task 3: S2 — 流式拖拽上传

**Files:**
- Create: `core/src/api/manage.rs`, `core/tests/upload_test.rs`
- Create: `clients/apple/Sources/MusicApp/Model/UploadViewModel.swift`, `clients/apple/Sources/MusicApp/Views/UploadView.swift`, `clients/apple/Tests/MusicAppTests/UploadViewModelTests.swift`
- Modify: `core/src/client.rs`, `clients/apple/Sources/MusicApp/Views/LibraryView.swift`

**Interfaces:**
- Produces `upload_track(path, key, progress) -> Track`; `UploadProgress` reports `(sent_bytes, total_bytes)`.
- Produces `UploadViewModel.accept(urls:)` and `UploadViewModel.progressByURL`.

- [ ] **Step 1: Write a failing streaming upload test**

```rust
#[tokio::test]
async fn upload_reads_file_in_bounded_chunks_and_reports_progress() { /* server records multipart */ }
```

- [ ] **Step 2: Verify red**

Run: `cargo test --manifest-path core/Cargo.toml --test upload_test`

Expected: no upload method or progress callback exists.

- [ ] **Step 3: Implement reqwest's built-in file multipart part without an added stream crate**

```rust
let part = reqwest::multipart::Part::file(&path)
    .await
    .map_err(CoreError::file)?;
let form = reqwest::multipart::Form::new().text("key", key).part("file", part);
```

`reqwest` owns the internal `ReaderStream`; the core never reads the whole file or adds a direct `tokio-util` dependency.

- [ ] **Step 4: Verify green and manual drop upload**

Run: `cargo test --manifest-path core/Cargo.toml --test upload_test && swift test --package-path clients/apple --filter UploadViewModelTests`

Expected: content length is not collected into a `Vec<u8>`, progress reaches 100%, resulting track refreshes the library.

- [ ] **Step 5: Commit**

```bash
git add core clients/apple
git commit -m "feat(mac): 支持流式拖拽上传曲目"
```

### Task 4: S3/S4 — 标签覆盖、删除与移动

**Files:**
- Create: `core/tests/manage_test.rs`, `clients/apple/Sources/MusicApp/Model/TrackEditorViewModel.swift`, `clients/apple/Sources/MusicApp/Views/TagEditorView.swift`, `clients/apple/Tests/MusicAppTests/TrackEditorViewModelTests.swift`
- Modify: `core/src/api/manage.rs`, `core/src/client.rs`, `clients/apple/Sources/MusicApp/Views/AlbumDetailView.swift`

**Interfaces:**
- Produces `update_tags(id, TagUpdate)`, `delete_track(id)`, `move_track(id, key)`.
- Produces `TrackEditorViewModel.save(tags:)`, `delete()` and `move(to:)`.

- [ ] **Step 1: Write failing tests for query encoding and post-success refresh**

```rust
#[tokio::test]
async fn update_tags_uses_ext_endpoint_and_omits_unchanged_fields() { /* assert query */ }
```

```swift
func testDeleteRefreshesAlbumAfterCoreSucceeds() async { /* fake records deletion */ }
```

- [ ] **Step 2: Verify red**

Run: `cargo test --manifest-path core/Cargo.toml --test manage_test && swift test --package-path clients/apple --filter TrackEditorViewModelTests`

Expected: no management methods or editor model.

- [ ] **Step 3: Implement exact `/rest/ext/{updateTags,deleteTrack,moveTrack}` calls and confirmation UI**

```rust
pub async fn delete_track(&self, id: String) -> Result<()> {
    self.manage.delete_track(&id).await
}
```

- [ ] **Step 4: Verify green**

Run: `cargo test --manifest-path core/Cargo.toml --test manage_test && swift test --package-path clients/apple --filter TrackEditorViewModelTests`

Expected: tags modify only server overlay, destructive operations only refresh after success.

- [ ] **Step 5: Commit**

```bash
git add core clients/apple
git commit -m "feat(mac): 支持标签编辑删除与移动"
```

### Task 5: S5 — 扫描状态

**Files:**
- Create: `core/src/api/scan.rs`, `core/tests/scan_test.rs`, `clients/apple/Sources/MusicApp/Model/ScanStatusViewModel.swift`, `clients/apple/Sources/MusicApp/Views/ScanStatusView.swift`, `clients/apple/Tests/MusicAppTests/ScanStatusViewModelTests.swift`
- Modify: `core/src/client.rs`, `clients/apple/Sources/MusicApp/Views/LibraryView.swift`

**Interfaces:**
- Produces `start_scan(prefix: Option<String>) -> ScanStatus` and `scan_status() -> ScanStatus`.
- Produces `ScanStatusViewModel.start()` and `refresh()`; view owns a cancellable periodic task while visible.

- [ ] **Step 1: Write failing scan decoding and polling tests**

```rust
#[tokio::test]
async fn scan_status_decodes_scanning_and_count() { /* JSON mock */ }
```

```swift
func testRefreshPublishesCurrentScanStatus() async { /* fake scanning true */ }
```

- [ ] **Step 2: Verify red**

Run: `cargo test --manifest-path core/Cargo.toml --test scan_test && swift test --package-path clients/apple --filter ScanStatusViewModelTests`

Expected: missing scan methods.

- [ ] **Step 3: Implement standard and prefix scan calls plus bounded UI polling**

```swift
pollTask = Task { [weak self] in
    while !Task.isCancelled { await self?.refresh(); try? await Task.sleep(for: .seconds(1)) }
}
```

- [ ] **Step 4: Verify green**

Run: `cargo test --manifest-path core/Cargo.toml --test scan_test && swift test --package-path clients/apple --filter ScanStatusViewModelTests`

Expected: view reports progress and cancels polling when it disappears.

- [ ] **Step 5: Commit**

```bash
git add core clients/apple
git commit -m "feat(mac): 触发并监控曲库扫描"
```

### Task 6: S6 — 封面显示与替换

**Files:**
- Create: `server/src/api/ext/cover.rs`, `core/src/api/media.rs`, `core/tests/media_test.rs`, `clients/apple/Sources/MusicApp/Model/CoverViewModel.swift`, `clients/apple/Sources/MusicApp/Views/CoverView.swift`, `clients/apple/Tests/MusicAppTests/CoverViewModelTests.swift`
- Modify: `server/src/api/ext/mod.rs`, `server/src/api/system.rs`, `server/tests/ext_test.rs`, `core/src/client.rs`, `clients/apple/Sources/MusicApp/Views/AlbumDetailView.swift`, `openapi.yaml`

**Interfaces:**
- Produces `set_cover_art(album_id, local_path) -> ()` and `cover_art_url(cover_key, size) -> String`.
- Produces `CoverViewModel.replace(with:)`, with `AsyncImage(url:)` consuming only the core URL.

- [ ] **Step 1: Write a failing server integration test before endpoint code**

```rust
#[tokio::test]
async fn set_cover_art_requires_admin_stores_image_and_updates_album_cover_key() { /* multipart request */ }
```

- [ ] **Step 2: Verify red**

Run: `cargo test --manifest-path server/Cargo.toml --test ext_test set_cover_art`

Expected: route returns 404.

- [ ] **Step 3: Implement `/rest/ext/setCoverArt` streaming temp-file upload, Garage put, transactional cover association and extension declaration**

```rust
Router::new().route("/rest/ext/setCoverArt", post(set_cover_art))
```

The handler validates image type and associated album/track before changing metadata, streams in bounded chunks, and removes a newly written Garage object if association fails.

- [ ] **Step 4: Write and run failing then green client/UI tests**

Run: `cargo test --manifest-path core/Cargo.toml --test media_test && swift test --package-path clients/apple --filter CoverViewModelTests`

Expected: URL carries auth parameters safely percent-encoded; replacement refreshes displayed cover only after a successful core result.

- [ ] **Step 5: Commit**

```bash
git add server core clients/apple openapi.yaml
git commit -m "feat(media): 支持替换曲库封面"
```

### Task 7: S7 — AVFoundation 试听与 M1 验证

**Files:**
- Create: `clients/apple/Sources/MusicApp/Audio/PreviewPlayer.swift`, `clients/apple/Tests/MusicAppTests/PreviewPlayerTests.swift`
- Modify: `core/src/api/media.rs`, `core/src/client.rs`, `clients/apple/Sources/MusicApp/Views/AlbumDetailView.swift`, `README.md`

**Interfaces:**
- Produces `stream_url(track_id, format, max_bitrate) -> String`.
- Produces `PreviewPlayer.play(url:)`, `stop()`, and `isPlaying`.

- [ ] **Step 1: Write failing URL construction and player-delegation tests**

```rust
#[test]
fn stream_url_includes_track_and_optional_transcode_parameters() { /* assert URL */ }
```

```swift
func testPlayReplacesThePreviousItem() { /* inject fake AVPlayer factory */ }
```

- [ ] **Step 2: Verify red**

Run: `cargo test --manifest-path core/Cargo.toml --test media_test && swift test --package-path clients/apple --filter PreviewPlayerTests`

Expected: stream URL and player are absent.

- [ ] **Step 3: Implement URL-only core method and AVPlayer wrapper**

```swift
func play(url: URL) { player.replaceCurrentItem(with: AVPlayerItem(url: url)); player.play() }
```

- [ ] **Step 4: Run complete validation and real-server M1 flow**

Run: `cargo test --manifest-path contract/Cargo.toml && cargo test --manifest-path core/Cargo.toml && cargo test --manifest-path server/Cargo.toml && cargo clippy --manifest-path core/Cargo.toml --all-targets -- -D warnings && cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings && cargo fmt --manifest-path core/Cargo.toml --check && cargo fmt --manifest-path server/Cargo.toml --check && swift test --package-path clients/apple`

Expected: all tests, lint and format checks pass. With `docker compose up`, log in as an admin and exercise upload → browse/search → tag edit → move/delete → scan → cover replacement → preview play.

- [ ] **Step 5: Commit**

```bash
git add clients/apple core README.md
git commit -m "feat(mac): 完成管理客户端 M1 试听闭环"
```
