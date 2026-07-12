# Mac 曲库浏览优化设计（并行任务 B）

## 1. 目标

提升曲库浏览体验：专辑排序切换、按流派/年份筛选、封面网格视图。服务端 `getAlbumList2` 已支持 `type=byGenre&genre=` 与 `type=byYear&fromYear=&toYear=`，`getGenres` 已实现——本任务在 core 暴露这些能力，并在 mac 曲库区实现浏览 UI。

## 2. 范围与并行边界

- **独占文件**：`clients/apple/Sources/Yevune/Views/LibraryView.swift`（仅其曲库/`.library` 详情区与专辑列表部分）、`clients/apple/Sources/Yevune/Model/LibraryViewModel.swift`，新建 `clients/apple/Sources/Yevune/Views/AlbumGridView.swift`；core 侧 `core/src/api/browse.rs`、`core/src/client.rs`、`clients/apple/Sources/Yevune/Model/LoginViewModel.swift`（`MusicClientProviding` 协议）、`clients/apple/Sources/Yevune/Model/CoreMusicClient.swift`；及各自测试。
- **不得改动**（归并行任务 A）：`MediaDetailView.swift`、`TagEditorView.swift`、`TagEditorViewModel.swift`。**尤其不要改 `MediaDetailView` 的初始化签名或其调用点的传参形状**——任务 A 依赖它稳定。若在网格里换用新的选择/进入详情方式，保持对 `MediaDetailView(album:model:playlists:)` 的调用签名不变。
- **不改 server**：服务端能力已就绪，仅 core 暴露 + Swift UI。
- **不做**：曲目级操作（属任务 A）、艺人筛选（本版仅流派/年份，除非顺带无成本）。

## 3. core

- **扩展专辑列表筛选**：在 `browse.rs` 让 `list_albums` 支持三态查询——排序（现有 5 种 `AlbumSort`）、按流派（`type=byGenre&genre=`）、按年份区间（`type=byYear&fromYear=&toYear=`）。建议引入表达查询意图的类型（如 `AlbumQuery`/`AlbumFilter` 枚举：`Sort(AlbumSort)` / `Genre(String)` / `YearRange { from: u32, to: u32 }`），或新增并列方法；由实现计划定形，保持 `uniffi` 可表达。`getAlbumList2` 对 `byYear` 要求 `fromYear`+`toYear`、对 `byGenre` 要求 `genre`，缺参服务端报错，core 侧保证必填。
- **新增 `list_genres() -> Vec<Genre>`**：走 `getGenres`（服务端已实现，返回 `{"genres":{"genre":[...]}}`）。`contract::Genre` 已存在，经 `ffi_types.rs` 的 `#[uniffi::remote(Record)]` 暴露（若尚未暴露则补）。
- `MusicClientProviding` 协议 + `CoreMusicClient` 桥接新方法（带 throwing 默认实现，保持既有 fake 免改）。重建 UniFFI 绑定（`clients/apple/Packages/YevuneCoreFFI/scripts/build-core.sh`），验证生成的 `YevuneCoreFFI.swift` 含新方法。

## 4. Swift UI

- **`AlbumGridView`**：`LazyVGrid` 封面网格（`AsyncImage` 取封面，占位灰块），标题 + 艺人；点击进入专辑详情（复用现有 selection→`MediaDetailView` 路径，签名不变）。
- **`LibraryView` 曲库详情区**：顶部浏览工具条——排序 `Picker`（5 种）、流派 `Picker`（来自 `list_genres()`，含「全部」）、年份区间输入/滑块（可选启用）、视图切换（网格/列表）。切换任一条件后经 `LibraryViewModel` 重新加载。
- **`LibraryViewModel`**：持有 `sort`、`genreFilter`、`yearRange`、`viewMode`、`genres` 列表；`load()` 依当前筛选调对应 core 方法；暴露 `errorMessage`、`isLoading`。保留现有 `albums`/`search` 行为。

## 5. 错误处理

- 加载/筛选/流派列表失败置 `errorMessage` 并呈现，保留上一次成功结果不清空为空白（避免筛选失败后空屏）。
- 年份区间非法（from>to 或空）在客户端拦截或按服务端报错提示。

## 6. 测试

- **core（TDD）**：用既有 `TcpListener` mock（见 `core/tests/*_test.rs` 模式）覆盖——`list_albums` 三种查询（sort / byGenre / byYear）的请求参数编码（`type=`、`genre=`、`fromYear=`/`toYear=`）与响应解码；`list_genres` 解码 `{"genres":{"genre":[...]}}`。
- **XCTest**：`LibraryViewModel` 用 mock client 覆盖——切换 sort/genre/year 触发正确的 client 调用与 `albums` 刷新；流派列表加载；错误态保留旧结果。
- 网格视图以 `swift build` 编译 + 手动冒烟为准。
- 全量 `cargo test`/`clippy -D warnings`/`fmt --check` 与 `swift test` 通过。

## 7. 架构边界

浏览筛选的查询编排与 DTO 解码在 Rust core；Swift `LibraryViewModel` 只协调筛选状态、`AlbumGridView`/`LibraryView` 只渲染。共享 DTO 复用 `contract`，不在客户端重复定义。不破坏 OpenSubsonic 兼容性（`getAlbumList2`/`getGenres` 为标准端点）。
