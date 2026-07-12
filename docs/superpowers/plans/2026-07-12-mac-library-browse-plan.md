# Mac 曲库浏览优化实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 core 暴露专辑排序/流派/年份筛选与流派列表查询，并在 mac 曲库区实现排序/流派/年份筛选工具条 + 封面网格视图，复用现有 `MediaDetailView` 详情路径。

**Architecture:** Rust core 新增 `AlbumFilter`（uniffi::Enum：`Sort`/`Genre`/`YearRange`）表达 `getAlbumList2` 的三态互斥查询，`list_albums` 改签名消费该枚举；新增 `list_genres` 桥接 `getGenres`。Swift 侧 `MusicClientProviding` 新增两个协议方法（默认 throwing 实现，既有假客户端免改），`LibraryViewModel` 持有筛选状态并据此选出 `AlbumFilter`；`LibraryView` 新增浏览工具条与网格/列表切换，新建 `AlbumGridView` 渲染封面网格，选中后仍走 `MediaDetailView(album:model:playlists:)`。

**Tech Stack:** Rust (`uniffi`, `sqlx` 服务端不动)、Swift/SwiftUI、XCTest、既有 `TcpListener` mock 测试模式。

## Global Constraints

- 不改 server；`getAlbumList2`/`getGenres` 参数与响应形状以 [browsing.rs](../../../server/src/api/browsing.rs) 为准：`type`（camelCase 下就是 `type`）、`fromYear`/`toYear`、`genre`；`byYear` 要求 `fromYear`+`toYear` 同时提供，`byGenre` 要求 `genre`。
- 并行安全边界（见任务描述）：只改 `core/src/api/browse.rs`、`core/src/client.rs`、`core/src/ffi_types.rs`、`core/src/lib.rs`、`core/tests/browse_test.rs`；Swift 侧只改 `LibraryView.swift`（曲库/`.library` 详情区）、`LibraryViewModel.swift`、新建 `AlbumGridView.swift`、`LoginViewModel.swift`（`MusicClientProviding` 协议部分）、`CoreMusicClient.swift`，新建 `LibraryViewModelTests.swift`。**绝不改** `MediaDetailView.swift`、`TagEditorView.swift`、`TagEditorViewModel.swift`、`LoginViewModelTests.swift`、`PlaylistViewModelTests.swift`。
- 新协议方法必须在 `MusicClientProviding` 扩展里提供 throwing 默认实现，保证 `LoginViewModelTests.swift`/`PlaylistViewModelTests.swift` 里的既有 Fake 客户端不用改也能编译。
- 错误态：加载/筛选/流派失败置 `errorMessage`，不清空上次成功的 `albums`。
- 完成后需：`cargo test`、`cargo clippy -- -D warnings`、`cargo fmt --check`、重建 UniFFI 绑定、`cd clients/apple && swift build && swift test` 全绿。

---

### Task 1: core `AlbumFilter` 枚举 + `list_albums` 筛选支持

**Files:**
- Modify: `core/src/api/browse.rs`
- Modify: `core/src/lib.rs`（re-export `AlbumFilter`）
- Modify: `core/src/client.rs`（`list_albums` 签名改为消费 `AlbumFilter`）
- Test: `core/tests/browse_test.rs`

**Interfaces:**
- Consumes: 现有 `AlbumSort`（`browse.rs:11`）、`HttpClient::get_json`（`http.rs`）。
- Produces: `pub enum AlbumFilter { Sort(AlbumSort), Genre(String), YearRange { from: u32, to: u32 } }`（`uniffi::Enum`）；`pub(crate) async fn list_albums(http, auth, filter: AlbumFilter, offset: u32, size: u32) -> Result<Vec<Album>>`；`MusicClient::list_albums(&self, filter: AlbumFilter, offset: u32, size: u32) -> Result<Vec<contract::Album>>`。后续任务（Swift 桥接）依赖此签名与 `AlbumFilter` 三个 case 名。

- [ ] **Step 1: 写失败测试——按流派查询编码 `type=byGenre&genre=`**

在 `core/tests/browse_test.rs` 末尾追加（复用文件内已有的 `use` 与 `response_for`，并扩展 `response_for` 支持 `getGenres`/`byGenre`/`byYear` 分支）：

```rust
async fn spin_server(n: usize) -> (std::net::SocketAddr, Arc<Mutex<Vec<String>>>, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let observed = requests.clone();
    let server = tokio::spawn(async move {
        for _ in 0..n {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = vec![0; 4096];
            let bytes = socket.read(&mut request).await.unwrap();
            let line = std::str::from_utf8(&request[..bytes])
                .unwrap()
                .lines()
                .next()
                .unwrap()
                .to_owned();
            observed.lock().await.push(line.clone());
            let body = response_for(&line);
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        }
    });
    (address, requests, server)
}

#[tokio::test]
async fn list_albums_by_genre_sends_type_and_genre_query() {
    let (address, requests, server) = spin_server(2).await;
    let client = MusicClient::new();
    client
        .login(format!("http://{address}"), "admin".to_owned(), "secret".to_owned())
        .await
        .unwrap();
    let albums = client
        .list_albums(AlbumFilter::Genre("Rock".to_owned()), 0, 50)
        .await
        .unwrap();
    server.await.unwrap();

    assert_eq!(albums[0].name, "Blue");
    let requests = requests.lock().await;
    assert!(requests[1].contains("/rest/getAlbumList2?"));
    assert!(requests[1].contains("type=byGenre"));
    assert!(requests[1].contains("genre=Rock"));
}

#[tokio::test]
async fn list_albums_by_year_range_sends_from_and_to_year() {
    let (address, requests, server) = spin_server(2).await;
    let client = MusicClient::new();
    client
        .login(format!("http://{address}"), "admin".to_owned(), "secret".to_owned())
        .await
        .unwrap();
    let albums = client
        .list_albums(AlbumFilter::YearRange { from: 2000, to: 2020 }, 0, 50)
        .await
        .unwrap();
    server.await.unwrap();

    assert_eq!(albums[0].name, "Blue");
    let requests = requests.lock().await;
    assert!(requests[1].contains("type=byYear"));
    assert!(requests[1].contains("fromYear=2000"));
    assert!(requests[1].contains("toYear=2020"));
}
```

同时把顶部 `use music_core::{AlbumSort, MusicClient};` 改为 `use music_core::{AlbumFilter, AlbumSort, MusicClient};`，并把原有测试里的 `client.list_albums(AlbumSort::Newest, 0, 50)` 改成 `client.list_albums(AlbumFilter::Sort(AlbumSort::Newest), 0, 50)`。

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test --manifest-path core/Cargo.toml list_albums_by_genre_sends_type_and_genre_query`
Expected: FAIL（编译错误，`AlbumFilter` 不存在 / `list_albums` 签名不匹配）

- [ ] **Step 3: 最小实现**

在 `core/src/api/browse.rs` 的 `AlbumSort` 定义之后加入：

```rust
/// `getAlbumList2` 的查询意图：三态互斥——按排序、按流派、按年份区间。
#[derive(Clone, uniffi::Enum)]
pub enum AlbumFilter {
    /// 按既有排序方式浏览。
    Sort(AlbumSort),
    /// 按流派筛选（对应 `type=byGenre&genre=`）。
    Genre(String),
    /// 按年份区间筛选，闭区间（对应 `type=byYear&fromYear=&toYear=`）。
    YearRange {
        /// 起始年份（含）。
        from: u32,
        /// 结束年份（含）。
        to: u32,
    },
}

impl AlbumFilter {
    fn query_params(&self) -> Vec<(String, String)> {
        match self {
            Self::Sort(sort) => vec![("type".to_owned(), sort.endpoint_value().to_owned())],
            Self::Genre(genre) => vec![
                ("type".to_owned(), "byGenre".to_owned()),
                ("genre".to_owned(), genre.clone()),
            ],
            Self::YearRange { from, to } => vec![
                ("type".to_owned(), "byYear".to_owned()),
                ("fromYear".to_owned(), from.to_string()),
                ("toYear".to_owned(), to.to_string()),
            ],
        }
    }
}
```

把 `list_albums` 改为：

```rust
pub(crate) async fn list_albums(
    http: &HttpClient,
    auth: &AuthenticatedSession,
    filter: AlbumFilter,
    offset: u32,
    size: u32,
) -> Result<Vec<Album>> {
    let mut params = filter.query_params();
    params.push(("offset".to_owned(), offset.to_string()));
    params.push(("size".to_owned(), size.to_string()));
    let payload: AlbumListPayload = http.get_json(auth, "getAlbumList2", &params).await?;
    Ok(payload.album_list2.album)
}
```

在 `core/src/client.rs` 顶部 `use crate::api::browse::{self, AlbumDetail, AlbumSort, ArtistDetail, SearchResult};` 改为 `use crate::api::browse::{self, AlbumDetail, AlbumFilter, AlbumSort, ArtistDetail, SearchResult};`，并把 `list_albums` 方法签名改为：

```rust
pub async fn list_albums(
    &self,
    filter: AlbumFilter,
    offset: u32,
    size: u32,
) -> Result<Vec<contract::Album>> {
    browse::list_albums(&self.http, &self.authenticated_session().await?, filter, offset, size).await
}
```

在 `core/src/lib.rs` 把 `pub use api::browse::{AlbumDetail, AlbumSort, ArtistDetail, SearchResult};` 改为 `pub use api::browse::{AlbumDetail, AlbumFilter, AlbumSort, ArtistDetail, SearchResult};`。

在 `response_for` 里补充 `byGenre`/`byYear` 都走 `getAlbumList2` 分支（已有分支按 URL 前缀匹配 `/rest/getAlbumList2` 即可覆盖，无需新增分支）。

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test --manifest-path core/Cargo.toml --test browse_test`
Expected: PASS（含年份区间测试）

- [ ] **Step 5: 提交**

```bash
git add core/src/api/browse.rs core/src/client.rs core/src/lib.rs core/tests/browse_test.rs
git commit -m "feat(core): 支持按流派/年份区间筛选专辑列表"
```

---

### Task 2: core `list_genres` + `Genre` FFI 暴露

**Files:**
- Modify: `core/src/api/browse.rs`
- Modify: `core/src/client.rs`
- Modify: `core/src/ffi_types.rs`
- Test: `core/tests/browse_test.rs`

**Interfaces:**
- Consumes: `contract::Genre { value: String, song_count: u32, album_count: u32 }`（`contract/src/media.rs`）。
- Produces: `pub(crate) async fn list_genres(http, auth) -> Result<Vec<contract::Genre>>`；`MusicClient::list_genres(&self) -> Result<Vec<contract::Genre>>`；`contract::Genre` 经 `#[uniffi::remote(Record)]` 可跨 FFI。

- [ ] **Step 1: 写失败测试**

在 `core/tests/browse_test.rs` 追加：

```rust
#[tokio::test]
async fn list_genres_decodes_genre_array() {
    let (address, requests, server) = spin_server(2).await;
    let client = MusicClient::new();
    client
        .login(format!("http://{address}"), "admin".to_owned(), "secret".to_owned())
        .await
        .unwrap();
    let genres = client.list_genres().await.unwrap();
    server.await.unwrap();

    assert_eq!(genres[0].value, "Rock");
    assert_eq!(genres[0].song_count, 5);
    assert_eq!(genres[0].album_count, 2);
    let requests = requests.lock().await;
    assert!(requests[1].contains("/rest/getGenres"));
}
```

在 `response_for` 里加一个分支（`else if request.contains("/rest/getGenres")`），返回：

```rust
"\"genres\":{\"genre\":[{\"value\":\"Rock\",\"songCount\":5,\"albumCount\":2}]}"
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test --manifest-path core/Cargo.toml list_genres_decodes_genre_array`
Expected: FAIL（`list_genres` 不存在）

- [ ] **Step 3: 最小实现**

`core/src/api/browse.rs` 顶部 `use contract::{Album, Artist, Track};` 改为 `use contract::{Album, Artist, Genre, Track};`，并新增：

```rust
pub(crate) async fn list_genres(http: &HttpClient, auth: &AuthenticatedSession) -> Result<Vec<Genre>> {
    let payload: GenresPayload = http.get_json(auth, "getGenres", &[]).await?;
    Ok(payload.genres.genre)
}

#[derive(Deserialize)]
struct GenresPayload {
    genres: GenresList,
}

#[derive(Deserialize)]
struct GenresList {
    #[serde(default)]
    genre: Vec<Genre>,
}
```

`core/src/client.rs` 新增：

```rust
/// 读取所有可见流派。
pub async fn list_genres(&self) -> Result<Vec<contract::Genre>> {
    browse::list_genres(&self.http, &self.authenticated_session().await?).await
}
```

`core/src/ffi_types.rs` 顶部 `use contract::{Album, Artist, Playlist, PlaylistFolder, Track};` 改为 `use contract::{Album, Artist, Genre, Playlist, PlaylistFolder, Track};`，并新增：

```rust
#[uniffi::remote(Record)]
pub struct Genre {
    pub value: String,
    pub song_count: u32,
    pub album_count: u32,
}
```

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test --manifest-path core/Cargo.toml --test browse_test`
Expected: PASS（全部 browse_test 用例，含新增 3 个）

- [ ] **Step 5: 提交**

```bash
git add core/src/api/browse.rs core/src/client.rs core/src/ffi_types.rs core/tests/browse_test.rs
git commit -m "feat(core): 新增 list_genres 桥接 getGenres"
```

---

### Task 3: 全量 core 校验 + 重建 UniFFI 绑定

**Files:**
- No source changes; runs `clients/apple/Packages/CoreFFI/scripts/build-core.sh`.

**Interfaces:**
- Consumes: Task 1/2 产出的 `AlbumFilter`、`list_genres`、`Genre`。
- Produces: 重新生成的 `clients/apple/Packages/CoreFFI/Sources/CoreFFI/CoreFFI.swift`，供 Task 4 使用（`AlbumFilter` 枚举、`Genre` 记录、`MusicClient.listAlbums(filter:offset:size:)`、`MusicClient.listGenres()`）。

- [ ] **Step 1: 跑 core 测试/lint 全量**

Run: `cargo test --manifest-path core/Cargo.toml && cargo clippy --manifest-path core/Cargo.toml -- -D warnings && cargo fmt --manifest-path core/Cargo.toml --check`
Expected: 全部通过

- [ ] **Step 2: 重建绑定**

Run: `./clients/apple/Packages/CoreFFI/scripts/build-core.sh`
Expected: 脚本成功结束，生成 `clients/apple/Packages/CoreFFI/Sources/CoreFFI/CoreFFI.swift`

- [ ] **Step 3: 验证生成文件包含新符号**

Run: `grep -n "enum AlbumFilter\|struct Genre\|func listGenres\|func listAlbums" clients/apple/Packages/CoreFFI/Sources/CoreFFI/CoreFFI.swift`
Expected: 输出包含 `AlbumFilter`、`Genre`、`listGenres`、两个 `listAlbums` 重载

- [ ] **Step 4: 提交生成产物**

```bash
git add clients/apple/Packages/CoreFFI/Sources/CoreFFI clients/apple/Packages/CoreFFI/MusicCoreFFI.xcframework
git commit -m "chore(mac): 重建 UniFFI 绑定以暴露专辑筛选与流派接口"
```

---

### Task 4: Swift 协议 + `CoreMusicClient` 桥接

**Files:**
- Modify: `clients/apple/Sources/MusicApp/Model/LoginViewModel.swift`（仅 `MusicClientProviding` 协议与扩展部分）
- Modify: `clients/apple/Sources/MusicApp/Model/CoreMusicClient.swift`

**Interfaces:**
- Consumes: `CoreFFI.AlbumFilter`、`CoreFFI.Genre`、`CoreFFI.MusicClient.listAlbums(filter:offset:size:)`、`CoreFFI.MusicClient.listGenres()`（Task 3 产出）。
- Produces: `MusicClientProviding.listAlbums(filter:offset:size:) async throws -> [Album]`、`MusicClientProviding.listGenres() async throws -> [Genre]`（默认 throwing 实现）。Task 5/6 依赖这两个方法名与签名。

- [ ] **Step 1: 在协议里新增方法声明**

在 `LoginViewModel.swift` 的 `protocol MusicClientProviding` 内，紧跟 `func listAlbums(offset: UInt32, size: UInt32) async throws -> [Album]` 之后新增：

```swift
    func listAlbums(filter: AlbumFilter, offset: UInt32, size: UInt32) async throws -> [Album]
    func listGenres() async throws -> [Genre]
```

在紧邻的 `extension MusicClientProviding` 里新增默认实现（保持既有 Fake 客户端免改）：

```swift
    func listAlbums(filter: AlbumFilter, offset: UInt32, size: UInt32) async throws -> [Album] { throw CocoaError(.featureUnsupported) }
    func listGenres() async throws -> [Genre] { throw CocoaError(.featureUnsupported) }
```

- [ ] **Step 2: 编译确认（此时 `swift build` 应因 `CoreMusicClient` 未实现新方法而非必须——新方法有默认实现，不会报错）**

Run: `cd clients/apple && swift build`
Expected: PASS（默认实现已满足协议要求）

- [ ] **Step 3: `CoreMusicClient` 提供真实桥接**

在 `CoreMusicClient.swift` 的 `listAlbums(offset:size:)` 之后新增：

```swift
    func listAlbums(filter: AlbumFilter, offset: UInt32, size: UInt32) async throws -> [Album] {
        try await client.listAlbums(filter: filter, offset: offset, size: size)
    }

    func listGenres() async throws -> [Genre] {
        try await client.listGenres()
    }
```

- [ ] **Step 4: 编译确认**

Run: `cd clients/apple && swift build`
Expected: PASS

- [ ] **Step 5: 提交**

```bash
git add clients/apple/Sources/MusicApp/Model/LoginViewModel.swift clients/apple/Sources/MusicApp/Model/CoreMusicClient.swift
git commit -m "feat(mac): 桥接专辑筛选与流派列表 core 接口"
```

---

### Task 5: `LibraryViewModel` 筛选状态（TDD）

**Files:**
- Modify: `clients/apple/Sources/MusicApp/Model/LibraryViewModel.swift`
- Test: `clients/apple/Tests/MusicAppTests/LibraryViewModelTests.swift`（新建）

**Interfaces:**
- Consumes: `MusicClientProviding.listAlbums(filter:offset:size:)`、`.listGenres()`（Task 4）。
- Produces: `LibraryViewModel` 新增 `@Published var sort: AlbumSort`、`genreFilter: String?`、`yearFilterEnabled: Bool`、`fromYear/toYear: UInt32`、`viewMode: LibraryViewMode`、`genres: [Genre]`（只读）、`func loadGenres() async`。Task 6（UI）依赖这些属性名。

- [ ] **Step 1: 新建测试文件，写失败测试**

创建 `clients/apple/Tests/MusicAppTests/LibraryViewModelTests.swift`：

```swift
import XCTest
import CoreFFI
@testable import MusicApp

@MainActor
final class LibraryViewModelTests: XCTestCase {
    func testLoadUsesSortFilterByDefault() async {
        let fake = FakeBrowseClient(albums: [albumFixture(id: "al-1", name: "Blue")])
        let model = LibraryViewModel(client: fake)

        await model.load()

        XCTAssertEqual(fake.lastFilter, .sort(.newest))
        XCTAssertEqual(model.albums.map(\.id), ["al-1"])
        XCTAssertNil(model.errorMessage)
    }

    func testSettingGenreFilterSwitchesToGenreQuery() async {
        let fake = FakeBrowseClient()
        let model = LibraryViewModel(client: fake)
        model.genreFilter = "Rock"

        await model.load()

        XCTAssertEqual(fake.lastFilter, .genre("Rock"))
    }

    func testEnablingYearRangeSwitchesToYearQuery() async {
        let fake = FakeBrowseClient()
        let model = LibraryViewModel(client: fake)
        model.yearFilterEnabled = true
        model.fromYear = 2000
        model.toYear = 2010

        await model.load()

        XCTAssertEqual(fake.lastFilter, .yearRange(from: 2000, to: 2010))
    }

    func testLoadGenresPublishesGenreList() async {
        let fake = FakeBrowseClient(genres: [Genre(value: "Rock", songCount: 5, albumCount: 2)])
        let model = LibraryViewModel(client: fake)

        await model.loadGenres()

        XCTAssertEqual(model.genres.first?.value, "Rock")
        XCTAssertNil(model.errorMessage)
    }

    func testLoadFailureKeepsPreviousAlbumsAndSetsError() async {
        let fake = FakeBrowseClient(albums: [albumFixture(id: "al-1", name: "Blue")])
        let model = LibraryViewModel(client: fake)
        await model.load()

        fake.shouldFail = true
        await model.load()

        XCTAssertEqual(model.albums.map(\.id), ["al-1"])
        XCTAssertNotNil(model.errorMessage)
    }
}

private func albumFixture(id: String, name: String) -> Album {
    Album(id: id, name: name, artist: nil, artistId: nil, coverArt: nil,
          songCount: 0, duration: 0, year: nil, genre: nil, created: nil)
}

/// 记录最近一次筛选调用的假客户端，其余方法走协议默认实现。
private final class FakeBrowseClient: MusicClientProviding, @unchecked Sendable {
    var albums: [Album]
    var genres: [Genre]
    var shouldFail = false
    private(set) var lastFilter: AlbumFilter?

    init(albums: [Album] = [], genres: [Genre] = []) {
        self.albums = albums
        self.genres = genres
    }

    func login(server: String, user: String, password: String) async throws -> SessionValue {
        SessionValue(server: server, user: user)
    }
    func listAlbums(offset: UInt32, size: UInt32) async throws -> [Album] { albums }
    func search(query: String) async throws -> SearchResult {
        SearchResult(artists: [], albums: albums, tracks: [])
    }
    func upload(localPath: String, libraryKey: String, progress: UploadProgress) async throws -> Track {
        throw CocoaError(.featureUnsupported)
    }
    func updateTags(id: String, update: TagUpdate) async throws {}
    func deleteTrack(id: String) async throws {}
    func moveTrack(id: String, key: String) async throws {}
    func startScan() async throws -> ScanStatus { throw CocoaError(.featureUnsupported) }
    func scanStatus() async throws -> ScanStatus { throw CocoaError(.featureUnsupported) }

    func listAlbums(filter: AlbumFilter, offset: UInt32, size: UInt32) async throws -> [Album] {
        if shouldFail { throw CocoaError(.fileReadUnknown) }
        lastFilter = filter
        return albums
    }
    func listGenres() async throws -> [Genre] {
        if shouldFail { throw CocoaError(.fileReadUnknown) }
        return genres
    }
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cd clients/apple && swift test --filter LibraryViewModelTests`
Expected: FAIL（编译错误：`LibraryViewModel` 无 `genreFilter`/`yearFilterEnabled`/`loadGenres` 等）

- [ ] **Step 3: 最小实现**

把 `LibraryViewModel.swift` 整体改为：

```swift
import CoreFFI
import Foundation

enum LibraryViewMode: String, CaseIterable, Identifiable {
    case grid = "网格"
    case list = "列表"
    var id: String { rawValue }
}

@MainActor
final class LibraryViewModel: ObservableObject {
    @Published private(set) var albums: [Album] = []
    @Published private(set) var searchResult: SearchResult?
    @Published private(set) var errorMessage: String?
    @Published private(set) var isLoading = false
    @Published private(set) var genres: [Genre] = []

    @Published var sort: AlbumSort = .newest
    @Published var genreFilter: String?
    @Published var yearFilterEnabled = false
    @Published var fromYear: UInt32 = 2000
    @Published var toYear: UInt32 = UInt32(Calendar.current.component(.year, from: Date()))
    @Published var viewMode: LibraryViewMode = .grid

    private let client: any MusicClientProviding
    var clientForViews: any MusicClientProviding { client }

    init(client: any MusicClientProviding) {
        self.client = client
    }

    func load() async {
        isLoading = true
        errorMessage = nil
        defer { isLoading = false }
        do {
            albums = try await client.listAlbums(filter: currentFilter, offset: 0, size: 100)
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func loadGenres() async {
        do {
            genres = try await client.listGenres()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func search(query: String) async {
        guard !query.isEmpty else {
            searchResult = nil
            return
        }
        isLoading = true
        errorMessage = nil
        defer { isLoading = false }
        do {
            searchResult = try await client.search(query: query)
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func album(id: String?) -> Album? {
        guard let id else { return nil }
        return albums.first { $0.id == id }
    }

    private var currentFilter: AlbumFilter {
        if let genre = genreFilter { return .genre(genre) }
        if yearFilterEnabled { return .yearRange(from: fromYear, to: toYear) }
        return .sort(sort)
    }
}
```

- [ ] **Step 4: 跑测试确认通过**

Run: `cd clients/apple && swift test --filter LibraryViewModelTests`
Expected: PASS（5 个测试）

- [ ] **Step 5: 跑既有 `LoginViewModelTests`/`PlaylistViewModelTests` 确认未破坏**

Run: `cd clients/apple && swift test --filter LoginViewModelTests && swift test --filter PlaylistViewModelTests`
Expected: PASS（既有 Fake 客户端因协议默认实现无需改动）

- [ ] **Step 6: 提交**

```bash
git add clients/apple/Sources/MusicApp/Model/LibraryViewModel.swift clients/apple/Tests/MusicAppTests/LibraryViewModelTests.swift
git commit -m "feat(mac): LibraryViewModel 支持排序/流派/年份筛选状态"
```

---

### Task 6: `AlbumGridView` + `LibraryView` 浏览工具条

**Files:**
- Create: `clients/apple/Sources/MusicApp/Views/AlbumGridView.swift`
- Modify: `clients/apple/Sources/MusicApp/Views/LibraryView.swift`（仅 `libraryDetail` 及相关 `@State`/`.task`）

**Interfaces:**
- Consumes: `LibraryViewModel.sort/genreFilter/yearFilterEnabled/fromYear/toYear/viewMode/genres`（Task 5）、`MusicClientProviding.coverArtURL(id:size:)`（既有）。
- Produces: 无新增可复用符号；以编译 + 手动冒烟验证。

- [ ] **Step 1: 新建 `AlbumGridView.swift`**

```swift
import CoreFFI
import SwiftUI

struct AlbumGridView: View {
    let albums: [Album]
    let client: any MusicClientProviding
    let newAlbumIDs: Set<String>
    let onSelect: (Album) -> Void

    private let columns = [GridItem(.adaptive(minimum: 150), spacing: 20)]

    var body: some View {
        ScrollView {
            LazyVGrid(columns: columns, spacing: 24) {
                ForEach(albums, id: \.id) { album in
                    AlbumGridCell(album: album, client: client, isNew: newAlbumIDs.contains(album.id))
                        .onTapGesture { onSelect(album) }
                }
            }
            .padding()
        }
    }
}

private struct AlbumGridCell: View {
    let album: Album
    let client: any MusicClientProviding
    let isNew: Bool
    @State private var coverURL: URL?

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            AsyncImage(url: coverURL) { image in
                image.resizable().scaledToFill()
            } placeholder: {
                Color.secondary.opacity(0.15)
            }
            .frame(width: 150, height: 150)
            .clipped()
            .cornerRadius(8)

            HStack {
                Text(album.name).font(.subheadline.bold()).lineLimit(1)
                if isNew {
                    Text("新增").font(.caption2).padding(.horizontal, 5).background(.green.opacity(0.2), in: Capsule())
                }
            }
            Text(album.artist ?? "未知艺人").font(.caption).foregroundStyle(.secondary).lineLimit(1)
        }
        .frame(width: 150)
        .task(id: album.id) {
            if let urlString = try? await client.coverArtURL(id: album.id, size: 300), let url = URL(string: urlString) {
                coverURL = url
            }
        }
    }
}
```

- [ ] **Step 2: 编译确认新文件独立可编译**

Run: `cd clients/apple && swift build`
Expected: PASS

- [ ] **Step 3: 在 `LibraryView.swift` 加浏览工具条与网格/列表切换**

把 `libraryDetail` 计算属性替换为（`selectedAlbumID`/`model`/`workflow`/`playlists`/`media`/`query` 均为既有 `@State`/`@StateObject`，不新增）：

```swift
    @ViewBuilder private var libraryDetail: some View {
        VStack(spacing: 0) {
            browseToolbar
            Divider()
            HStack(spacing: 0) {
                Group {
                    if model.viewMode == .grid {
                        AlbumGridView(
                            albums: model.albums,
                            client: model.clientForViews,
                            newAlbumIDs: workflow.newAlbumIDs,
                            onSelect: { selectedAlbumID = $0.id }
                        )
                    } else {
                        List(model.albums, id: \.id, selection: $selectedAlbumID) { album in
                            VStack(alignment: .leading, spacing: 3) {
                                HStack {
                                    Text(album.name).font(.headline)
                                    if workflow.newAlbumIDs.contains(album.id) {
                                        Text("新增").font(.caption2).padding(.horizontal, 5).background(.green.opacity(0.2), in: Capsule())
                                    }
                                }
                                Text(album.artist ?? "未知艺人")
                                    .font(.subheadline)
                                    .foregroundStyle(.secondary)
                            }
                            .tag(album.id)
                        }
                    }
                }
                .frame(minWidth: 260, idealWidth: 340, maxWidth: model.viewMode == .grid ? .infinity : 320)

                Divider()

                if let selection = model.album(id: selectedAlbumID) {
                    MediaDetailView(album: selection, model: media, playlists: playlists)
                } else {
                    VStack(spacing: 18) {
                        TextField("搜索艺人、专辑或曲目", text: $query)
                            .textFieldStyle(.roundedBorder)
                            .onSubmit { Task { await model.search(query: query) } }
                        if let result = model.searchResult {
                            List(result.albums, id: \.id) { album in
                                Text(album.name)
                            }
                        } else if model.isLoading {
                            ProgressView("正在加载曲库…")
                        } else if let errorMessage = model.errorMessage {
                            Text(errorMessage).foregroundStyle(.red)
                        } else {
                            Text("选择专辑以查看曲目")
                                .foregroundStyle(.secondary)
                        }
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .padding()
                }
            }
        }
        .task { await model.loadGenres() }
    }

    @ViewBuilder private var browseToolbar: some View {
        HStack(spacing: 16) {
            Picker("排序", selection: $model.sort) {
                Text("最近入库").tag(AlbumSort.newest)
                Text("按专辑名").tag(AlbumSort.alphabeticalByName)
                Text("按艺人名").tag(AlbumSort.alphabeticalByArtist)
                Text("最常播放").tag(AlbumSort.frequent)
                Text("最近播放").tag(AlbumSort.recent)
            }
            .frame(maxWidth: 160)
            .disabled(model.genreFilter != nil || model.yearFilterEnabled)

            Picker("流派", selection: genreBinding) {
                Text("全部").tag(String?.none)
                ForEach(model.genres, id: \.value) { genre in
                    Text(genre.value).tag(String?.some(genre.value))
                }
            }
            .frame(maxWidth: 160)

            Toggle("按年份", isOn: $model.yearFilterEnabled)
            if model.yearFilterEnabled {
                Stepper("从 \(model.fromYear)", value: yearBinding(\.fromYear), in: 1900...2100)
                Stepper("到 \(model.toYear)", value: yearBinding(\.toYear), in: 1900...2100)
            }

            Spacer()

            Picker("视图", selection: $model.viewMode) {
                ForEach(LibraryViewMode.allCases) { mode in
                    Text(mode.rawValue).tag(mode)
                }
            }
            .pickerStyle(.segmented)
            .frame(maxWidth: 160)
        }
        .padding()
        .onChange(of: model.sort) { _, _ in Task { await model.load() } }
        .onChange(of: model.genreFilter) { _, _ in Task { await model.load() } }
        .onChange(of: model.yearFilterEnabled) { _, _ in Task { await model.load() } }
        .onChange(of: model.fromYear) { _, _ in if model.yearFilterEnabled { Task { await model.load() } } }
        .onChange(of: model.toYear) { _, _ in if model.yearFilterEnabled { Task { await model.load() } } }
    }

    private var genreBinding: Binding<String?> {
        Binding(get: { model.genreFilter }, set: { model.genreFilter = $0 })
    }

    private func yearBinding(_ keyPath: ReferenceWritableKeyPath<LibraryViewModel, UInt32>) -> Binding<UInt32> {
        Binding(get: { model[keyPath: keyPath] }, set: { model[keyPath: keyPath] = $0 })
    }
```

- [ ] **Step 4: 编译确认**

Run: `cd clients/apple && swift build`
Expected: PASS

- [ ] **Step 5: 手动冒烟（不写自动化 UI 测试，以 spec §6 为准）**

Run: `cd clients/apple && swift run MusicApp`（或通过 Xcode 运行），登录后在曲库区依次：切换排序 Picker、选流派、勾选年份区间、切换网格/列表，确认专辑列表随之刷新且网格封面可点击进入详情。

- [ ] **Step 6: 提交**

```bash
git add clients/apple/Sources/MusicApp/Views/AlbumGridView.swift clients/apple/Sources/MusicApp/Views/LibraryView.swift
git commit -m "feat(mac): 曲库浏览工具条与封面网格视图"
```

---

### Task 7: 全量校验

**Files:** 无新改动，仅运行校验。

- [ ] **Step 1:** `cargo test --manifest-path core/Cargo.toml`
- [ ] **Step 2:** `cargo clippy --manifest-path core/Cargo.toml -- -D warnings`
- [ ] **Step 3:** `cargo fmt --manifest-path core/Cargo.toml --check`
- [ ] **Step 4:** `cd clients/apple && swift build`
- [ ] **Step 5:** `cd clients/apple && swift test`

Expected: 全部 PASS，即完成的定义（DoD）达成。
