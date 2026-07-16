# Mac 曲库发现工作台 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 macOS 客户端交付可浏览 100 张以上专辑、支持艺人入口与三类独立分页搜索的现代化曲库发现工作台。

**Architecture:** Rust `core` 继续独占认证、OpenSubsonic 参数编码和响应解码，并新增 UniFFI 搜索分页记录；Swift 将浏览、搜索、艺人详情拆成三个 `@MainActor` 状态模型，以取消和 generation 双重隔离过期响应。SwiftUI 以 1180pt 为断点提供规则“收藏 + 检视器”和紧凑 `NavigationStack`，复用现有 `MediaDetailView` 与全局 `PlaybackController`。

**Tech Stack:** Rust 1.97、tokio、serde、UniFFI、Swift 5.9、SwiftUI、XCTest、macOS 14；不新增依赖。

## Global Constraints

- 权威规格：`docs/superpowers/specs/2026-07-16-mac-library-discovery-design.md`。
- 专辑页大小固定 60；搜索初始页及分类续页固定 24；搜索防抖固定 250ms。
- 规则布局断点为 `>= 1180pt`；应用最小窗口 920pt；不得使用 macOS 15 `ScrollPosition`。
- 只复用标准 OpenSubsonic `getAlbumList2`、`getArtists`、`getArtist`、`getAlbum`、`search3`；不修改 `contract` 或服务端。
- Swift 不复制网络、认证或参数编码；不新增依赖、图片缓存、持久数据库或播放器实例。
- 普通成员不得构造导入、扫描、任务入口；服务端授权仍是最终边界。
- 每项产品改动严格执行 RED → GREEN → REFACTOR，并按任务小步提交。

---

## File Responsibility Map

| File | Responsibility |
|---|---|
| `core/src/api/browse.rs` | 搜索分页 DTO、`search3` 六项分页参数、裁剪与 `has_more` |
| `core/src/client.rs` | UniFFI `search_page` 门面与分页上限前置校验 |
| `core/tests/browse_test.rs` | Rust 请求编码、边界和兼容测试 |
| `clients/apple/Sources/Yevune/Model/LoginViewModel.swift` | Swift 客户端协议的分页桥接接口 |
| `clients/apple/Sources/Yevune/Model/CoreMusicClient.swift` | 对生成 UniFFI API 的一对一转发 |
| `clients/apple/Sources/Yevune/Model/LibraryBrowseViewModel.swift` | 专辑分页、互斥条件、艺人集合、刷新错误与 generation |
| `clients/apple/Sources/Yevune/Model/LibrarySearchViewModel.swift` | 250ms 防抖、三类独立分页与 generation |
| `clients/apple/Sources/Yevune/Model/ArtistDetailViewModel.swift` | 单艺人详情请求与晚到隔离 |
| `clients/apple/Sources/Yevune/Views/Library/LibraryViewPolicy.swift` | 断点、命令栏和管理员入口纯策略 |
| `clients/apple/Sources/Yevune/Views/Library/*.swift` | 命令栏、专辑/艺人收藏、艺人详情、搜索结果和响应式容器 |
| `clients/apple/Sources/Yevune/Views/Playlist/PlaylistTreeOutline.swift` | 从根视图移出的既有歌单树渲染 |
| `clients/apple/Sources/Yevune/Views/LibraryView.swift` | 根路由、session 级覆盖层与播放器 safe-area |

### Task 1: Rust core 独立分页搜索

**Files:**
- Modify: `core/src/api/browse.rs`
- Modify: `core/src/client.rs`
- Modify: `core/src/lib.rs`
- Test: `core/tests/browse_test.rs`

**Interfaces:**
- Consumes: `HttpClient::get_json`、`AuthenticatedSession`、现有 `Artist`、`Album`、`Track`。
- Produces: `SearchPageRequest`、`SearchPage`、`MusicClient::search_page(SearchPageRequest) -> Result<SearchPage>`；保留 `search(String) -> Result<SearchResult>`。

- [ ] **Step 1: 写请求参数、裁剪、0/100/101 与兼容性的失败测试**

在 `core/tests/browse_test.rs` 的 mock server 测试中构造：

```rust
let request = SearchPageRequest {
    query: "blue".into(),
    artist_offset: 3,
    artist_count: 2,
    album_offset: 5,
    album_count: 1,
    track_offset: 7,
    track_count: 0,
};
let page = client.search_page(request).await.unwrap();
assert_eq!(page.artists.len(), 2);
assert!(page.has_more_artists);
assert_eq!(page.albums.len(), 1);
assert!(page.has_more_albums);
assert!(page.tracks.is_empty());
assert!(!page.has_more_tracks);
assert!(captured.contains("artistOffset=3"));
assert!(captured.contains("artistCount=3"));
assert!(captured.contains("albumOffset=5"));
assert!(captured.contains("albumCount=2"));
assert!(captured.contains("songOffset=7"));
assert!(captured.contains("songCount=0"));
```

另加 `count == 100` 请求 101、`count == 101` 在发网前返回 `CoreError::InvalidRequest`，以及旧 `search("blue")` 三类各返回最多 20 条的测试。非法上限测试必须验证 mock server 未收到请求。

- [ ] **Step 2: 运行定向测试并确认 RED**

Run: `cargo test --manifest-path core/Cargo.toml --test browse_test search_page -- --nocapture`

Expected: FAIL，原因是 `SearchPageRequest`、`SearchPage` 或 `search_page` 尚不存在。

- [ ] **Step 3: 在 `browse.rs` 实现分页记录和纯裁剪逻辑**

```rust
#[derive(Clone, uniffi::Record)]
pub struct SearchPageRequest {
    pub query: String,
    pub artist_offset: u32,
    pub artist_count: u32,
    pub album_offset: u32,
    pub album_count: u32,
    pub track_offset: u32,
    pub track_count: u32,
}

#[derive(Clone, uniffi::Record)]
pub struct SearchPage {
    pub artists: Vec<Artist>,
    pub albums: Vec<Album>,
    pub tracks: Vec<Track>,
    pub has_more_artists: bool,
    pub has_more_albums: bool,
    pub has_more_tracks: bool,
}

fn trim_page<T>(mut values: Vec<T>, count: u32) -> (Vec<T>, bool) {
    let limit = count as usize;
    let has_more = values.len() > limit;
    values.truncate(limit);
    (values, has_more)
}
```

`browse::search_page` 始终编码六个 offset/count；非零 count 使用 `count + 1`，零 count 原样编码 0。解码后分别调用 `trim_page`。现有 `browse::search` 调用分页函数，以三类 offset 0、count 20 构造旧 `SearchResult`。

- [ ] **Step 4: 在 `client.rs` 做认证前校验并导出方法**

```rust
pub async fn search_page(&self, request: SearchPageRequest) -> Result<SearchPage> {
    for count in [request.artist_count, request.album_count, request.track_count] {
        if count > 100 {
            return Err(CoreError::InvalidRequest("search count must be <= 100".into()));
        }
    }
    browse::search_page(&self.http, &self.authenticated_session().await?, request).await
}
```

同步更新 `use` 与 `core/src/lib.rs` 的公开 re-export，使 UniFFI 生成器可见两个记录。

- [ ] **Step 5: 跑绿并执行 core 门禁**

Run:

```bash
cargo test --manifest-path core/Cargo.toml --test browse_test
cargo test --manifest-path core/Cargo.toml
cargo clippy --manifest-path core/Cargo.toml -- -D warnings
cargo fmt --manifest-path core/Cargo.toml --check
```

Expected: 全部 PASS，clippy/fmt 退出码 0。

- [ ] **Step 6: 提交**

```bash
git add core/src/api/browse.rs core/src/client.rs core/src/lib.rs core/tests/browse_test.rs
git commit -m "feat(core): 支持曲库搜索独立分页"
```

### Task 2: Swift 协议与 UniFFI 桥接

**Files:**
- Modify: `clients/apple/Sources/Yevune/Model/LoginViewModel.swift`
- Modify: `clients/apple/Sources/Yevune/Model/CoreMusicClient.swift`
- Test: `clients/apple/Tests/YevuneTests/LoginViewModelTests.swift`

**Interfaces:**
- Consumes: Task 1 生成的 Swift `SearchPageRequest`、`SearchPage`、`MusicClient.searchPage(request:)`。
- Produces: `MusicClientProviding.searchPage(request:) async throws -> SearchPage`。

- [ ] **Step 1: 重新生成本地 UniFFI 绑定**

Run: `swift build --package-path clients/apple`

Expected: 构建成功，并在 SwiftPM 派生输出中生成 `SearchPageRequest`、`SearchPage`、`searchPage`；生成文件不加入 Git。

- [ ] **Step 2: 写协议桥接失败测试**

给 `LoginViewModelTests.swift` 的 spy 实现 `searchPage`，传入精确 request 后断言其记录的六项分页值与返回的三类 `hasMore` 不变。测试核心断言：

```swift
let request = SearchPageRequest(
    query: "blue", artistOffset: 1, artistCount: 24,
    albumOffset: 2, albumCount: 24,
    trackOffset: 3, trackCount: 24
)
let page = try await client.searchPage(request: request)
XCTAssertEqual(page.hasMoreAlbums, true)
XCTAssertEqual(await spy.lastSearchPageRequest?.albumOffset, 2)
```

- [ ] **Step 3: 运行并确认 RED**

Run: `swift test --package-path clients/apple --filter LoginViewModelTests`

Expected: FAIL，`MusicClientProviding` 尚无 `searchPage(request:)`。

- [ ] **Step 4: 最小实现协议与 production 转发**

在协议加入：

```swift
func searchPage(request: SearchPageRequest) async throws -> SearchPage
```

在协议 extension 提供默认抛错，避免无关测试 double 同步膨胀：

```swift
func searchPage(request: SearchPageRequest) async throws -> SearchPage {
    throw CocoaError(.featureUnsupported)
}
```

`CoreMusicClient` 必须显式一对一转发：

```swift
func searchPage(request: SearchPageRequest) async throws -> SearchPage {
    try await client.searchPage(request: request)
}
```

- [ ] **Step 5: 跑绿并确认绑定无手写副本**

Run:

```bash
swift test --package-path clients/apple --filter LoginViewModelTests
swift build --package-path clients/apple
rg -n "struct SearchPage(Request)?" clients/apple/Sources
```

Expected: 测试/构建 PASS；最后一条无输出，证明 Swift Sources 未复制 Rust DTO。

- [ ] **Step 6: 提交**

```bash
git add clients/apple/Sources/Yevune/Model/LoginViewModel.swift clients/apple/Sources/Yevune/Model/CoreMusicClient.swift clients/apple/Tests/YevuneTests/LoginViewModelTests.swift
git commit -m "feat(mac): 桥接曲库分页搜索接口"
```

### Task 3: 确定性的专辑与艺人浏览模型

**Files:**
- Create: `clients/apple/Sources/Yevune/Model/LibraryBrowseViewModel.swift`
- Create: `clients/apple/Tests/YevuneTests/LibraryBrowseViewModelTests.swift`
- Modify: `clients/apple/Sources/Yevune/Model/LibraryWorkflowViewModel.swift`
- Modify: `clients/apple/Tests/YevuneTests/LoginViewModelTests.swift`

**Interfaces:**
- Consumes: `MusicClientProviding.listAlbums(filter:offset:size:)`、`listGenres()`、`listArtists()`。
- Produces: `LibrarySection`、`AlbumBrowseCriterion`、`LibraryBrowseViewModel.reload()`、`loadNextPage()`、`selectSection(_:)`、`selectCriterion(_:)`。

- [ ] **Step 1: 写浏览状态的失败测试和可控客户端**

测试 double 用 actor 保存请求并用 continuation 控制返回顺序：

```swift
actor SuspendedLibraryClient: MusicClientProviding {
    struct AlbumCall { let filter: AlbumFilter; let offset: UInt32; let size: UInt32 }
    private var albumCalls: [AlbumCall] = []
    private var waiters: [CheckedContinuation<[Album], Error>] = []

    func listAlbums(filter: AlbumFilter, offset: UInt32, size: UInt32) async throws -> [Album] {
        albumCalls.append(.init(filter: filter, offset: offset, size: size))
        return try await withCheckedThrowingContinuation { waiters.append($0) }
    }

    func resolveAlbumCall(_ index: Int, with albums: [Album]) {
        waiters[index].resume(returning: albums)
    }
}
```

覆盖以下独立用例：初始请求 `(offset: 0, size: 60)`；60 张后请求 offset 60；第 3 页越过 100；重复 ID 顺序去重；连续两次 `loadNextPage` 只发一次；精确 60 张允许空尾页再关闭 `hasMoreAlbums`；genre/year/sort 完整替换；`from > to` 设置校验文案且不请求；刷新失败保留旧内容；下一页失败保留当前内容；旧筛选晚返回不覆盖新筛选的内容、错误或 loading；艺人按 `sortName ?? name` 排序。

- [ ] **Step 2: 运行并确认 RED**

Run: `swift test --package-path clients/apple --filter LibraryBrowseViewModelTests`

Expected: FAIL，目标文件与类型尚不存在。

- [ ] **Step 3: 实现互斥类型和 60 张分页骨架**

```swift
enum LibrarySection: String, CaseIterable { case albums, artists }

enum AlbumBrowseCriterion {
    case sort(AlbumSort)
    case genre(String)
    case yearRange(from: UInt32, to: UInt32)

    var filter: AlbumFilter? {
        switch self {
        case .sort(let value): return .sort(value)
        case .genre(let value): return .genre(value)
        case .yearRange(let from, let to):
            return from <= to ? .yearRange(from: from, to: to) : nil
        }
    }
}

@MainActor
final class LibraryBrowseViewModel: ObservableObject {
    static let albumPageSize: UInt32 = 60
    @Published private(set) var albums: [Album] = []
    @Published private(set) var artists: [Artist] = []
    @Published private(set) var genres: [Genre] = []
    @Published private(set) var hasMoreAlbums = true
    @Published private(set) var isRefreshing = false
    @Published private(set) var isLoadingNextPage = false
    @Published private(set) var initialError: String?
    @Published private(set) var refreshError: String?
    @Published private(set) var nextPageError: String?
    @Published private(set) var validationMessage: String?
    @Published private(set) var section: LibrarySection = .albums
    @Published private(set) var albumCriterion: AlbumBrowseCriterion = .sort(.newest)

    private let client: any MusicClientProviding
    private var requestTask: Task<Void, Never>?
    private var generation = 0
}
```

`reload()` 先递增 generation、取消旧 task，成功前不清空旧数组；响应回写前比较 generation 和捕获的 section/criterion。`loadNextPage()` guard `hasMoreAlbums && !isLoadingNextPage`，offset 使用 `UInt32(albums.count)`，通过 `Set(albums.map(\.id))` 稳定去重；少于 60 关闭 hasMore，等于 60 保持 true。

- [ ] **Step 4: 实现艺人加载、错误分层与工作流刷新**

`selectSection` 和 `selectCriterion` 只在值真正变化时启动 `reload()`；artists 分支调用一次 `listArtists()`，按下式排序：

```swift
artists = response.sorted {
    ($0.sortName ?? $0.name).localizedStandardCompare($1.sortName ?? $1.name) == .orderedAscending
}
```

首次且无内容的失败写 `initialError`；已有内容刷新失败写 `refreshError`；追加失败只写 `nextPageError`。把 `LibraryWorkflowViewModel` 的依赖从 `LibraryViewModel` 改为 `LibraryBrowseViewModel`，扫描成功后调用 `await library.reload()`。

- [ ] **Step 5: 跑绿并做竞态重复验证**

Run:

```bash
swift test --package-path clients/apple --filter LibraryBrowseViewModelTests
swift test --package-path clients/apple --filter LoginViewModelTests
for i in {1..20}; do swift test --package-path clients/apple --filter LibraryBrowseViewModelTests/testLateResponseCannotOverwriteNewCriterion >/dev/null || exit 1; done
```

Expected: 全部 PASS，20 轮无竞态失败。

- [ ] **Step 6: 提交**

```bash
git add clients/apple/Sources/Yevune/Model/LibraryBrowseViewModel.swift clients/apple/Sources/Yevune/Model/LibraryWorkflowViewModel.swift clients/apple/Tests/YevuneTests/LibraryBrowseViewModelTests.swift clients/apple/Tests/YevuneTests/LoginViewModelTests.swift
git commit -m "feat(mac): 建立确定性曲库分页状态"
```

### Task 4: 防抖与三类独立分页搜索模型

**Files:**
- Create: `clients/apple/Sources/Yevune/Model/LibrarySearchViewModel.swift`
- Create: `clients/apple/Tests/YevuneTests/LibrarySearchViewModelTests.swift`

**Interfaces:**
- Consumes: `MusicClientProviding.searchPage(request:)`。
- Produces: `LibrarySearchPhase`、`SearchResultCategory`、`LibrarySearchViewModel.setInput(_:)`、`retryInitial()`、`loadMore(_:)`、`clear()`。

- [ ] **Step 1: 写防抖、晚到和独立分页失败测试**

注入不依赖墙钟的 sleeper：

```swift
typealias SearchSleeper = @Sendable (Duration) async throws -> Void

let immediateSleeper: SearchSleeper = { duration in
    XCTAssertEqual(duration, .milliseconds(250))
}
```

用 suspended client 覆盖：空白输入立即 idle 且清空；输入只在 sleeper 完成后请求；A 请求晚于 B 返回不覆盖 B；初始 request 三类 count 均 24；`.albums` 续页只把 `albumOffset` 设为当前 album 数且另外两类 count 为 0；分类续页不替换其他分类；三类 ID 稳定去重；分类错误只写对应错误；清空后不服从取消的旧结果也不能回写。

- [ ] **Step 2: 运行并确认 RED**

Run: `swift test --package-path clients/apple --filter LibrarySearchViewModelTests`

Expected: FAIL，搜索模型和 phase 尚不存在。

- [ ] **Step 3: 实现 phase、generation 和初始请求**

```swift
enum LibrarySearchPhase: Equatable { case idle, debouncing, loading, results, empty, failed(String) }
enum SearchResultCategory: CaseIterable { case artists, albums, tracks }

@MainActor
final class LibrarySearchViewModel: ObservableObject {
    static let pageSize: UInt32 = 24
    @Published private(set) var input = ""
    @Published private(set) var query = ""
    @Published private(set) var phase: LibrarySearchPhase = .idle
    @Published private(set) var artists: [Artist] = []
    @Published private(set) var albums: [Album] = []
    @Published private(set) var tracks: [Track] = []
    @Published private(set) var hasMoreArtists = false
    @Published private(set) var hasMoreAlbums = false
    @Published private(set) var hasMoreTracks = false
    @Published private(set) var nextPageErrors: [SearchResultCategory: String] = [:]

    private let client: any MusicClientProviding
    private let sleeper: SearchSleeper
    private var task: Task<Void, Never>?
    private var generation = 0
}
```

`setInput` trim 后若为空调用 `clear()`；否则 generation + 1、cancel、phase `.debouncing`，sleep 250ms 后 phase `.loading` 并发三类各 24 的单次 `searchPage`。捕获的 generation/query 在任何 published 写入前同时校验。

- [ ] **Step 4: 实现分类续页、去重和错误恢复**

用一个纯 helper 保序去重：

```swift
private func appendingUnique<T>(_ current: [T], _ incoming: [T], id: (T) -> String) -> [T] {
    var seen = Set(current.map(id))
    return current + incoming.filter { seen.insert(id($0)).inserted }
}
```

`loadMore(.artists)` 构造 artist offset = artists.count/count = 24，其余 count = 0；albums/tracks 对称实现。请求成功只追加目标分类并更新该分类 `hasMore`；失败只写 `nextPageErrors[category]`，初始已成功分组不清空。

- [ ] **Step 5: 跑绿并做竞态重复验证**

Run:

```bash
swift test --package-path clients/apple --filter LibrarySearchViewModelTests
for i in {1..20}; do swift test --package-path clients/apple --filter LibrarySearchViewModelTests/testLateQueryCannotOverwriteCurrentQuery >/dev/null || exit 1; done
```

Expected: 全部 PASS，20 轮无竞态失败。

- [ ] **Step 6: 提交**

```bash
git add clients/apple/Sources/Yevune/Model/LibrarySearchViewModel.swift clients/apple/Tests/YevuneTests/LibrarySearchViewModelTests.swift
git commit -m "feat(mac): 加入防抖分页曲库搜索"
```

### Task 5: 艺人详情与纯布局/权限策略

**Files:**
- Create: `clients/apple/Sources/Yevune/Model/ArtistDetailViewModel.swift`
- Create: `clients/apple/Sources/Yevune/Views/Library/LibraryViewPolicy.swift`
- Create: `clients/apple/Tests/YevuneTests/ArtistDetailViewModelTests.swift`
- Create: `clients/apple/Tests/YevuneTests/LibraryViewPolicyTests.swift`

**Interfaces:**
- Consumes: `MusicClientProviding.getArtist(id:)`、`SessionValue.admin`。
- Produces: `ArtistDetailViewModel.load(artistID:)`、`LibraryViewPolicy.layout(for:)`、`commandBarItems(compact:)`、`managementActions(isAdmin:)`、`artistSectionTitle(_:)`。

- [ ] **Step 1: 写艺人晚到隔离与策略失败测试**

```swift
func testLayoutBreaksAt1180Points() {
    XCTAssertEqual(LibraryViewPolicy.layout(for: 1_179), .compact)
    XCTAssertEqual(LibraryViewPolicy.layout(for: 1_180), .regular)
}

func testMembersConstructNoManagementActions() {
    XCTAssertEqual(LibraryViewPolicy.managementActions(isAdmin: false), [])
    XCTAssertEqual(
        LibraryViewPolicy.managementActions(isAdmin: true),
        [.importMusic, .scanLibrary, .showTasks]
    )
}
```

另测 compact 命令栏只有 `.section/.search/.filter`；regular 增加 `.summary/.viewStyle`；拉丁字母艺人按大写首字母分区，中文、数字和符号归 `#`；先请求 artist A 再请求 B，A 晚到不覆盖 B。

- [ ] **Step 2: 运行并确认 RED**

Run: `swift test --package-path clients/apple --filter 'ArtistDetailViewModelTests|LibraryViewPolicyTests'`

Expected: FAIL，两个 production 类型尚不存在。

- [ ] **Step 3: 实现艺人详情 generation 边界**

```swift
@MainActor
final class ArtistDetailViewModel: ObservableObject {
    @Published private(set) var detail: ArtistDetail?
    @Published private(set) var isLoading = false
    @Published private(set) var errorMessage: String?
    private let client: any MusicClientProviding
    private var task: Task<Void, Never>?
    private var generation = 0

    func load(artistID: String) {
        generation += 1
        let expected = generation
        task?.cancel()
        task = Task { [weak self] in
            guard let self else { return }
            isLoading = true
            errorMessage = nil
            do {
                let value = try await client.getArtist(id: artistID)
                guard expected == generation else { return }
                detail = value
                isLoading = false
            } catch {
                guard expected == generation else { return }
                errorMessage = error.localizedDescription
                isLoading = false
            }
        }
    }
}
```

- [ ] **Step 4: 实现无 UI 副作用的策略**

```swift
enum LibraryLayout: Equatable { case compact, regular }
enum LibraryCommandItem: Equatable { case section, search, filter, summary, viewStyle }
enum LibraryManagementAction: Equatable { case importMusic, scanLibrary, showTasks }

enum LibraryViewPolicy {
    static func layout(for width: CGFloat) -> LibraryLayout { width >= 1_180 ? .regular : .compact }
    static func commandBarItems(compact: Bool) -> [LibraryCommandItem] {
        compact ? [.section, .search, .filter] : [.section, .search, .summary, .filter, .viewStyle]
    }
    static func managementActions(isAdmin: Bool) -> [LibraryManagementAction] {
        isAdmin ? [.importMusic, .scanLibrary, .showTasks] : []
    }
    static func artistSectionTitle(_ artist: Artist) -> String {
        let source = artist.sortName ?? artist.name
        guard let scalar = source.trimmingCharacters(in: .whitespacesAndNewlines).unicodeScalars.first,
              scalar.isASCII, CharacterSet.letters.contains(scalar) else { return "#" }
        return String(scalar).uppercased()
    }
}
```

- [ ] **Step 5: 跑绿**

Run: `swift test --package-path clients/apple --filter 'ArtistDetailViewModelTests|LibraryViewPolicyTests'`

Expected: 全部 PASS。

- [ ] **Step 6: 提交**

```bash
git add clients/apple/Sources/Yevune/Model/ArtistDetailViewModel.swift clients/apple/Sources/Yevune/Views/Library/LibraryViewPolicy.swift clients/apple/Tests/YevuneTests/ArtistDetailViewModelTests.swift clients/apple/Tests/YevuneTests/LibraryViewPolicyTests.swift
git commit -m "feat(mac): 建立艺人详情与曲库布局策略"
```

### Task 6: 自适应 SwiftUI 曲库界面

**Required skill:** 开始本任务前完整阅读并遵循 `frontend-design:frontend-design`；它决定视觉取舍，但不得改变本计划的数据与权限边界。

**Files:**
- Create: `clients/apple/Sources/Yevune/Views/Library/LibraryCommandBar.swift`
- Create: `clients/apple/Sources/Yevune/Views/Library/AlbumCollectionView.swift`
- Create: `clients/apple/Sources/Yevune/Views/Library/ArtistCollectionView.swift`
- Create: `clients/apple/Sources/Yevune/Views/Library/ArtistDetailView.swift`
- Create: `clients/apple/Sources/Yevune/Views/Library/LibrarySearchResultsView.swift`
- Create: `clients/apple/Sources/Yevune/Views/Library/LibraryBrowserView.swift`
- Create: `clients/apple/Tests/YevuneTests/LibraryPresentationTests.swift`

**Interfaces:**
- Consumes: Tasks 3–5 的三个 view model 和 `LibraryViewPolicy`；现有 `MediaDetailView`、`AuthenticatedArtworkView`、`PlaybackController`。
- Produces: `LibraryBrowserView`，由调用方显式注入共享 browse/search/artist-detail 模型、client、playback 和 session；不得在 view 内新建 `CoreMusicClient`。

- [ ] **Step 1: 写可验证的呈现合同失败测试**

让 production view 消费 `LibraryViewPolicy`，并测试其纯呈现合同：

```swift
func testCompactPresentationFitsMinimumWindow() {
    let presentation = LibraryPresentation(width: 920, isAdmin: false)
    XCTAssertEqual(presentation.layout, .compact)
    XCTAssertEqual(presentation.commandItems, [.section, .search, .filter])
    XCTAssertEqual(presentation.managementActions, [])
}

func testRegularPresentationUsesInspector() {
    XCTAssertEqual(LibraryPresentation(width: 1_280, isAdmin: true).layout, .regular)
}
```

另测空曲库文案分别为管理员“导入音乐”和成员“曲库尚无音乐，请联系管理员添加”；搜索空态含实际 query 和“清除搜索”；详情返回 action 只改变 navigation selection，不触发 playback API。

- [ ] **Step 2: 运行并确认 RED**

Run: `swift test --package-path clients/apple --filter LibraryPresentationTests`

Expected: FAIL，`LibraryPresentation` 与新视图尚不存在。

- [ ] **Step 3: 实现命令栏与收藏视图**

`LibraryCommandBar` 使用 `searchable`/原生 `TextField` 支持 Command-F；compact 只渲染 section、search、filter，年份 Stepper 与网格/列表选项进入 popover。`AlbumCollectionView` 使用：

```swift
LazyVGrid(columns: [GridItem(.adaptive(minimum: 156, maximum: 190), spacing: 18)], spacing: 22)
```

封面始终复用 `AuthenticatedArtworkView`，标题最多两行；滚动尾部按 `hasMoreAlbums/isLoadingNextPage/nextPageError` 显示加载或重试。`ArtistCollectionView` 使用 `LibraryViewPolicy.artistSectionTitle` 分区，姓名首字符作为无封面占位，并保留键盘焦点、Return、双击和 VoiceOver label。

- [ ] **Step 4: 实现艺人详情和三类搜索结果**

`ArtistDetailView` 展示艺人标题及 `detail.albums`，选择专辑调用同一详情路由。`LibrarySearchResultsView` 必须直接读取 `LibrarySearchViewModel` 的 artists/albums/tracks 和三个 hasMore/error；艺人与专辑为紧凑横向收藏，曲目为列表。曲目 Return/双击调用现有搜索结果队列播放方法，不创建预览播放器。

- [ ] **Step 5: 实现 1180pt 自适应容器**

`GeometryReader` 把可用宽度传给 `LibraryViewPolicy.layout`：regular 使用主收藏加 380–480pt inspector；compact 使用 macOS 14 `NavigationStack` 推进艺人/专辑详情。导航标题为“返回曲库，继续播放”，返回只修改 path。清空搜索恢复浏览 view 本身，确保查询条件和 SwiftUI 滚动树仍在。

- [ ] **Step 6: 跑绿、构建和静态禁用项检查**

Run:

```bash
swift test --package-path clients/apple --filter LibraryPresentationTests
swift build --package-path clients/apple
rg -n "AsyncImage|ScrollPosition|CoreMusicClient\(" clients/apple/Sources/Yevune/Views/Library
```

Expected: 测试/构建 PASS；最后一条无输出。

- [ ] **Step 7: 提交**

```bash
git add clients/apple/Sources/Yevune/Views/Library clients/apple/Tests/YevuneTests/LibraryPresentationTests.swift
git commit -m "feat(mac): 构建自适应曲库发现界面"
```

### Task 7: 根视图集成与旧曲库职责移除

**Files:**
- Modify: `clients/apple/Sources/Yevune/App.swift`
- Modify: `clients/apple/Sources/Yevune/Views/LibraryView.swift`
- Modify: `clients/apple/Sources/Yevune/Model/LibraryWorkflowViewModel.swift`
- Create: `clients/apple/Sources/Yevune/Views/Playlist/PlaylistTreeOutline.swift`
- Modify: `clients/apple/Tests/YevuneTests/LoginViewModelTests.swift`
- Delete: `clients/apple/Sources/Yevune/Model/LibraryViewModel.swift`
- Delete: `clients/apple/Tests/YevuneTests/LibraryViewModelTests.swift`
- Delete: `clients/apple/Sources/Yevune/Views/AlbumGridView.swift`
- Delete: `clients/apple/Tests/YevuneTests/AlbumGridViewTests.swift`
- Delete: `clients/apple/Sources/Yevune/Views/Playback/SearchPlaybackResults.swift`

**Interfaces:**
- Consumes: `LibraryBrowserView` 和 Task 3–5 的共享模型。
- Produces: 登录后唯一曲库入口；保留既有歌单、管理、导入、任务抽屉和播放器 safe-area 行为。

- [ ] **Step 1: 写根装配和权限守卫失败测试**

在 `LoginViewModelTests.swift` 增加 app graph 测试 helper，并断言同一个 client 被传给 browse/search/artist detail/workflow；同一个 `LibraryBrowseViewModel` 被 workflow 与 browser 共享。通过 `LibraryPresentation` 断言 member graph 的管理 action 数为 0、admin graph 为 3。现有 workflow 扫描测试改为断言 `LibraryBrowseViewModel.reload()` 产生 size 60 的刷新请求。

- [ ] **Step 2: 运行并确认 RED**

Run: `swift test --package-path clients/apple --filter LoginViewModelTests`

Expected: FAIL，App 仍装配旧 `LibraryViewModel`。

- [ ] **Step 3: 在 App 建立一次性共享模型**

登录 session 建立后，用同一个 `CoreMusicClient` 创建并持有：

```swift
let browseModel = LibraryBrowseViewModel(client: client)
let searchModel = LibrarySearchViewModel(client: client)
let artistDetailModel = ArtistDetailViewModel(client: client)
let workflowModel = LibraryWorkflowViewModel(client: client, library: browseModel)
```

把这些实例显式注入 `LibraryView`/`LibraryBrowserView`，禁止 view body 重建模型或 client。

- [ ] **Step 4: 拆出歌单树并替换曲库 detail**

把 `LibraryView.swift` 现有歌单 outline 渲染原样移动到 `Views/Playlist/PlaylistTreeOutline.swift`，不改变 CRUD 或 drag/drop。曲库 detail 替换为 `LibraryBrowserView`；根视图只保留路由、session 级 sheet/overlay、player safe-area 和管理员工具栏连接。

- [ ] **Step 5: 落实管理员 UI 构造守卫并删除旧实现**

根视图仅遍历：

```swift
LibraryViewPolicy.managementActions(isAdmin: session.admin)
```

来构造导入、扫描、任务按钮；member 分支中不得预先创建再 `.hidden()`。删除旧 `LibraryViewModel`、`AlbumGridView`、`SearchPlaybackResults` 及对应测试；所有搜索播放交给 `LibrarySearchResultsView`。

- [ ] **Step 6: 全量 Swift 验证与竞态重复跑**

Run:

```bash
swift test --package-path clients/apple
swift build --package-path clients/apple
for i in {1..20}; do swift test --package-path clients/apple --filter 'LibraryBrowseViewModelTests|LibrarySearchViewModelTests|ArtistDetailViewModelTests' >/dev/null || exit 1; done
rg -n "LibraryViewModel|AlbumGridView|SearchPlaybackResults" clients/apple/Sources clients/apple/Tests
```

Expected: 测试/构建和 20 轮重复测试 PASS；最后一条无输出。

- [ ] **Step 7: 提交**

```bash
git add -A clients/apple
git commit -m "feat(mac): 集成曲库发现工作台"
```

### Task 8: 真实大曲库冒烟、全仓门禁与审查

**Files:**
- Create: `.superpowers/sdd/m4-library-discovery-report.md`

**Interfaces:**
- Consumes: 完整 M4 产品。
- Produces: 可复核的本地真实服务、125 张以上专辑、920/1280pt UI、权限与播放连续性验收记录。

- [ ] **Step 1: 启动真实本地 Garage 与 Rust server**

按仓库已有本地开发脚本启动依赖，记录命令、服务端 commit、端口和健康检查结果到报告。所有密码只通过当前 shell 的 `SMOKE_PASSWORD` 环境变量传入，报告与命令历史不得写凭证值。用标准 API 验证：

```bash
curl -fsS --get "http://127.0.0.1:4533/rest/ping.view" \
  --data-urlencode "u=admin" \
  --data-urlencode "p=$SMOKE_PASSWORD" \
  --data-urlencode "v=1.16.1" \
  --data-urlencode "c=yevune-m4" \
  --data-urlencode "f=json"
```

Expected: HTTP 200 且 OpenSubsonic response status 为 `ok`。

- [ ] **Step 2: 生成并导入至少 125 张唯一短 FLAC 专辑**

使用已安装 FFmpeg 生成 1 秒静音 FLAC，每个文件写唯一 album/title/artist 标签，通过现有上传/扫描接口导入。请求：

```bash
curl -fsS --get "http://127.0.0.1:4533/rest/getAlbumList2.view" \
  --data-urlencode "u=admin" \
  --data-urlencode "p=$SMOKE_PASSWORD" \
  --data-urlencode "v=1.16.1" \
  --data-urlencode "c=yevune-m4" \
  --data-urlencode "f=json" \
  --data-urlencode "type=alphabeticalByName" \
  --data-urlencode "offset=120" \
  --data-urlencode "size=60"
```

Expected: offset 120 返回至少 5 张专辑；把专辑总数、返回数和脱敏 response 摘要写入报告。

- [ ] **Step 3: 创建真实普通成员并验证服务端授权**

通过现有管理员 API 创建临时 member，分别登录 admin/member；记录 member 能浏览曲库但导入/扫描管理 API 被服务端拒绝。用户名可记录，密码只驻留环境变量且验收后删除临时用户。

- [ ] **Step 4: 打包临时 `.app` 并用本机 UI 做双宽度验收**

运行 `./scripts/tests/run-mac-client-test.sh` 生成/启动测试 app，并使用 Computer Use 技能检查 920pt 与 1280pt。逐项记录截图路径和结果：紧凑命令栏无裁剪；能滚过第 100 张；排序/流派/年份快速切换无闪回；三类搜索及分别续页；compact push/pop 恢复收藏；regular 收藏 + inspector；艺人分区/详情；浏览、搜索、详情和返回时播放持续；member 无管理入口而 admin 可导入/扫描/看任务。

- [ ] **Step 5: 若冒烟发现缺陷，先按系统化调试另起 RED 修复循环**

本计划不预设产品修复文件。任何冒烟缺陷必须先使用 `superpowers:systematic-debugging` 定位根因，在报告中记录精确失败测试和涉及文件，再以独立 RED → GREEN 提交修复；不得只改报告或绕过失败断言。

- [ ] **Step 6: 运行完整门禁**

Run:

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
```

Expected: 全部退出码 0；报告记录每条命令的时间和结果。

- [ ] **Step 7: 请求最终代码审查并处理反馈**

先完整阅读 `superpowers:requesting-code-review`，以基线 `fb1dc36` 到当前 HEAD 请求审查，重点检查：分页上限、旧响应隔离、member UI 构造守卫、920pt 布局、播放器单实例。若有反馈，按 `superpowers:receiving-code-review` 验证后修复并重跑受影响门禁。

- [ ] **Step 8: 提交验收报告**

```bash
git add .superpowers/sdd/m4-library-discovery-report.md
git commit -m "test(mac): 记录曲库发现工作台验收"
```

## Plan Self-Review

- Spec coverage: Tasks 1–2 覆盖 Rust/UniFFI 分页；Tasks 3–5 覆盖浏览、搜索、艺人、错误、断点与权限；Tasks 6–7 覆盖完整信息架构、视觉交互和根集成；Task 8 覆盖 125+ 专辑、双宽度、角色和播放连续性真实验收。
- Placeholder scan: 计划不含未定义的 TBD/TODO/“类似前项”或凭证占位；运行时秘密统一来自 `SMOKE_PASSWORD` 环境变量。
- Type consistency: `SearchPageRequest/SearchPage` 从 Task 1 贯穿 Task 2/4；`LibraryBrowseViewModel` 从 Task 3 贯穿 Task 6/7；`LibraryViewPolicy` 从 Task 5 贯穿 Task 6/7；页大小 60/24、防抖 250ms、断点 1180pt 全文一致。
- Scope: 不修改 server/contract，不改变播放、歌单、ACL 或上传协议，不新增依赖。
