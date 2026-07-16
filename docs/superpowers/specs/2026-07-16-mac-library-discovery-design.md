# Mac 曲库发现工作台设计

## 1. 目标

把现有“最多加载 100 张专辑、搜索藏在空白详情区、筛选状态可能互相覆盖”的曲库界面升级为适合日常使用的原生 macOS 工作台：大曲库可以完整浏览，搜索与筛选在快速操作时保持确定性，920pt 最小窗口不溢出，浏览和管理过程中播放不中断。

本阶段采用“唱片架 + 检视器”信息架构。宽屏同时显示收藏与详情；紧凑窗口一次只显示收藏或详情，返回时保留查询、选择和滚动位置。播放继续复用全局 `PlaybackController`，不建立第二套预览播放器。

## 2. 范围与非目标

### 2.1 本阶段包含

- 专辑分页浏览，移除前 100 张的实际数据上限。
- 专辑与艺人两种曲库入口。
- 统一、始终可见的搜索入口；结果包含艺人、专辑和曲目。
- 搜索三类结果的独立分页。
- 互斥且可解释的专辑浏览条件：排序、流派或年份。
- 920pt 与宽屏的自适应收藏/详情布局。
- 原生键盘操作、空态、错误恢复和加载下一页重试。
- 管理员专属的导入、扫描和任务入口守卫。
- 将 `LibraryView` 中曲库浏览职责拆入专注文件，根视图只负责路由和全局覆盖层。

### 2.2 本阶段不包含

- 不新增服务端端点；复用标准 OpenSubsonic `getAlbumList2`、`getArtists`、`getArtist`、`getAlbum` 与 `search3`。
- 不实现歌曲全集浏览；歌曲通过搜索、专辑和歌单进入。
- 不改变歌单树 CRUD、标签编辑、访问控制或上传扫描协议。
- 不增加离线缓存、下载、智能歌单或跨用户共享。
- 不改变播放队列、播放器 UI、音频引擎或系统媒体生命周期。
- 不引入第三方依赖、图片缓存服务或新的持久数据库。

## 3. 信息架构

左侧主侧栏继续包含：

1. 资料库：曲库。
2. 歌单：现有多级文件夹与歌单树。
3. 管理：管理员可见的用户、角色和访问控制。

选择“曲库”后，主区域由稳定命令栏与内容区组成。命令栏包含：

- `专辑 / 艺人` 分段选择。
- 全局曲库搜索框。
- 当前浏览条件摘要。
- “筛选”按钮。
- `网格 / 列表` 视图选择。

非空搜索词会把内容区切换为搜索结果，但不改变侧栏选择。清空搜索或按 `Esc` 返回此前的专辑/艺人位置。

### 3.1 专辑模式

专辑以自适应网格或列表呈现。选择专辑后复用现有 `MediaDetailView` 及其播放、标签、批量操作、加入歌单和访问控制能力。

专辑浏览条件是单一互斥状态，不再由多个布尔值隐式决定优先级：

- 最近入库。
- 按专辑名。
- 按艺人名。
- 最常播放。
- 最近播放。
- 指定流派。
- 年份闭区间。

流派和年份不能同时处于激活状态；选择新条件会完整替换旧条件。

### 3.2 艺人模式

艺人使用 `getArtists` 取得当前用户可见结果，按 `sortName ?? name` 的首字母分区。网格展示艺人封面、姓名与专辑数；无封面时使用姓名首字符的原生占位，不显示无意义的统一灰块。

选择艺人后通过 `getArtist` 展示艺人标题与其专辑收藏。选择其中专辑进入同一 `MediaDetailView`，不复制专辑详情或播放逻辑。

### 3.3 搜索模式

搜索结果按艺人、专辑、曲目分组：

- 艺人与专辑使用紧凑横向收藏区。
- 曲目使用高信息密度列表，支持双击/Return 播放和现有播放上下文菜单。
- 每组拥有独立的“继续加载”状态；加载一组不会清空或阻塞另外两组。

## 4. 响应式布局

应用继续支持最小主窗口宽度 920pt。

- `>= 1180pt`：规则布局。收藏区域与右侧检视器同时存在；检视器宽度在 380–480pt 之间，收藏获得其余宽度。
- `< 1180pt`：紧凑布局。收藏占满内容区；打开专辑或艺人后用详情替换收藏，并显示“返回曲库，继续播放”。

紧凑命令栏只常驻分段选择、搜索和筛选按钮。浏览摘要、排序细节与年份范围进入原生 popover；网格/列表切换可以放入同一 popover，不能靠压缩固定宽控件勉强容纳。

详情返回不暂停、停止或重建全局播放会话。紧凑布局使用 macOS 14 可用的 `NavigationStack` 路径推进详情，使收藏视图保留在导航历史中并自然恢复滚动位置；不得依赖 macOS 15 才提供的 `ScrollPosition`。返回后恢复此前的浏览条件、搜索词和选择。若查询改变后选中项不再可见，才关闭该详情。

## 5. 视觉与交互方向

界面最低支持 macOS 14，并保持原生系统字体、动态颜色、键盘焦点和 reduced-motion 行为。视觉主题来自音乐收藏本身，不使用通用渐变仪表盘或大面积玻璃卡片。

- 专辑网格卡片宽度在 156–190pt 间自适应，封面为正方形。
- 标题最多两行，艺人和年份为次级信息；长文本不得扩大卡片固定宽度。
- 选中态使用系统强调色边缘与轻量背景，不在封面上覆盖大面积颜色。
- 艺人列表的超大字母分区标记是本阶段唯一强调性视觉签名；其余区域保持克制。
- 初始加载使用稳定占位尺寸，避免封面到达后网格跳动。
- 播放中的曲目沿用当前扬声器状态，不新增第二套高亮语言。

键盘行为：

- `Command-F` 聚焦搜索框。
- `Esc` 依次清空搜索、关闭详情；均不影响播放。
- `Return` 打开选中的艺人/专辑，或播放选中的搜索曲目。
- 双击艺人/专辑打开详情；双击曲目播放其所在搜索结果队列。
- 网格与列表必须保留可见焦点、VoiceOver 标签及现有上下文菜单。

## 6. Rust core 搜索分页

现有 `list_albums(filter, offset, size)` 已满足专辑分页，不修改 `contract` 或服务端。

`core` 新增 UniFFI 自有记录：

```rust
pub struct SearchPageRequest {
    pub query: String,
    pub artist_offset: u32,
    pub artist_count: u32,
    pub album_offset: u32,
    pub album_count: u32,
    pub track_offset: u32,
    pub track_count: u32,
}

pub struct SearchPage {
    pub artists: Vec<Artist>,
    pub albums: Vec<Album>,
    pub tracks: Vec<Track>,
    pub has_more_artists: bool,
    pub has_more_albums: bool,
    pub has_more_tracks: bool,
}
```

`MusicClient::search_page(request) -> Result<SearchPage>` 使用标准 `search3` 的 `artistOffset/artistCount`、`albumOffset/albumCount` 和 `songOffset/songCount`。每类向服务端请求 `count + 1`，返回前裁掉多余项并计算 `has_more_*`。单类 count 为 0 时不加载该类。每类 count 最大 100；加一前使用有界校验，禁止整数溢出或超过服务端上限。

现有 `MusicClient::search(query)` 保留，内部使用每类 20 条的 `search_page`，返回原 `SearchResult`，维持现有调用方和其他平台兼容。

Swift `MusicClientProviding` 与 `CoreMusicClient` 一对一暴露 `searchPage(request:)`；网络、参数编码、认证和响应解码仍只存在于 Rust core。

## 7. Swift 状态模型

### 7.1 浏览状态

`LibraryBrowseViewModel` 取代现有 `LibraryViewModel` 中互相耦合的专辑、搜索与筛选字段，负责：

- `section: LibrarySection`（专辑或艺人）。
- `albumCriterion: AlbumBrowseCriterion`。
- `albums`、`artists`、`genres`。
- `isRefreshing`、`isLoadingNextPage`、`hasMoreAlbums`。
- `initialError`、`refreshError`、`nextPageError`。
- 选中专辑/艺人标识及当前查询 generation。

专辑页大小固定为 60。`reload()` 清空分页 offset 但在新请求成功前保留旧内容；第一次进入且无旧内容时显示初始加载态。`loadNextPage()` 仅在 `hasMoreAlbums && !isLoadingNextPage` 时执行，以已发布专辑数作为 offset，并按专辑 ID 去重。

每次改变 section 或 album criterion 都递增 generation 并取消旧 Task。所有响应在写入前同时核对 generation 和查询快照。旧请求晚到不得改变内容、错误或加载标记。

艺人使用现有 `listArtists()` 一次加载；同样受 generation 保护。艺人详情使用独立 `ArtistDetailViewModel`，按艺人 ID 隔离晚到结果。

### 7.2 搜索状态

`LibrarySearchViewModel` 独立持有：

- 原始输入和 trim 后查询。
- `idle / debouncing / loading / results / empty / failed` 阶段。
- 三类结果、各自 offset、`hasMore` 与下一页错误。
- 搜索 generation 和可取消 Task。

输入变化后等待 250ms 再发起初始搜索；清空输入立即取消并回到 idle。测试通过注入异步 sleeper 避免依赖真实墙钟。初始页每类 24 条；继续加载每次只请求对应类别 24 条，按 ID 去重并保留其他类别。

取消和 generation 必须同时使用：取消减少无效工作，generation 保证不服从取消的 FFI/网络结果也不能回写。

## 8. 错误、空态与权限

- 空曲库：管理员显示“导入音乐”；普通成员显示“曲库尚无音乐，请联系管理员添加”。
- 空搜索：显示实际关键词及“清除搜索”，不使用无上下文的通用空态。
- 初始失败：完整错误态与重试按钮。
- 刷新失败：保留最后成功内容，命令栏下方显示非阻塞横幅与重试。
- 下一页失败：只替换列表底部加载区，允许重试。
- 搜索初始失败：保留输入并提供重试；已加载分组的追加失败不清空结果。
- 年份起始值大于结束值时不发请求，筛选 popover 内显示明确校验文案。

`session.admin == false` 时不得构造导入、扫描和任务抽屉按钮；服务端授权仍是最终边界。管理员入口隐藏只用于避免普通成员触发注定失败的操作，不替代服务端校验。

## 9. 文件与职责边界

现有 `LibraryView.swift` 已同时承担路由、曲库、筛选、搜索、歌单树、管理入口和弹窗状态。本阶段按职责拆分：

- `LibraryView.swift`：根路由、全局导入覆盖层、播放器 safe-area 与 session 级工具栏。
- `Views/Library/LibraryBrowserView.swift`：曲库内容路由与规则/紧凑详情切换。
- `Views/Library/LibraryCommandBar.swift`：搜索、section、criterion 与布局控制。
- `Views/Library/AlbumCollectionView.swift`：分页专辑网格/列表及加载尾部。
- `Views/Library/ArtistCollectionView.swift`：字母分区艺人收藏。
- `Views/Library/ArtistDetailView.swift`：艺人标题与专辑集合。
- `Views/Library/LibrarySearchResultsView.swift`：三类分页搜索结果。
- `Views/Library/LibraryViewPolicy.swift`：1180pt 布局、紧凑命令栏和权限纯策略。
- `Views/Playlist/PlaylistTreeOutline.swift`：从根视图移出的现有歌单树渲染，不改变行为。
- `Model/LibraryBrowseViewModel.swift`：分页、筛选、艺人集合与 generation。
- `Model/LibrarySearchViewModel.swift`：防抖搜索与三类独立分页。
- `Model/ArtistDetailViewModel.swift`：单艺人详情请求边界。

`MediaDetailView`、`PlaybackController`、`PlaylistViewModel` 与访问控制模型保持既有公共行为。只在新视图调用它们，不复制业务逻辑。

## 10. 测试与完成定义

### 10.1 Rust core

- `search_page` 编码全部六个分页参数。
- 三类分别使用 count + 1，并正确裁剪与设置 `has_more`。
- count 0、count 100、非法超限与错误响应。
- 现有 `search(query)` 行为和返回类型保持兼容。

### 10.2 Swift 模型

- 60 张初始专辑与第二页追加，覆盖超过 100 张的场景。
- 精确 60 的倍数允许一次空尾页并最终关闭 `hasMore`。
- 重复专辑 ID 去重且顺序稳定。
- 同时触发两次筛选，旧请求后返回时不覆盖新结果或错误。
- 下一页重复触发只发送一次请求。
- 初始、刷新、下一页失败分别保留正确内容。
- 流派、年份、排序互斥；非法年份不请求。
- 艺人排序、分区和晚到详情隔离。
- 搜索 A 晚于搜索 B 返回时不覆盖 B。
- 三类搜索结果独立分页、去重和错误恢复。
- 清空搜索取消请求并清除旧结果。

### 10.3 Swift 视图与策略

- 920pt 使用紧凑布局，1180pt 使用规则布局。
- 紧凑命令栏不构造固定宽年份 Stepper。
- 规则/紧凑详情返回均不调用暂停或 shutdown。
- 搜索视图真实消费分页状态和三类结果。
- 普通成员不构造导入、扫描、任务按钮；管理员构造全部管理入口。
- 空态、刷新横幅、底部重试与 VoiceOver 文案可见。

### 10.4 真实冒烟与全仓门禁

使用本地 Garage + Rust 服务端准备至少 125 张专辑，并验证：

1. 滚动越过第 100 张仍能加载。
2. 快速切换排序、流派和年份不闪回旧结果。
3. 搜索艺人、专辑、曲目并分别继续加载。
4. 920pt 下命令栏、网格、紧凑详情和返回无裁剪。
5. 宽屏下收藏与检视器同时工作。
6. 浏览、搜索、详情和返回过程中播放连续。
7. 普通成员看不到管理入口，管理员仍可导入和扫描。

完成前必须通过：

- `swift test --package-path clients/apple`
- `swift build --package-path clients/apple`
- `cargo test --manifest-path contract/Cargo.toml`
- `cargo test --manifest-path server/Cargo.toml`
- `cargo test --manifest-path core/Cargo.toml`
- 三个 crate 的 `cargo clippy -- -D warnings`
- 三个 crate 的 `cargo fmt --check`
- `./scripts/tests/run-mac-client-test.sh`
- `git diff --check`

不得引入新依赖，不得破坏 OpenSubsonic 兼容性，不得把网络分页或认证逻辑复制到 Swift。
