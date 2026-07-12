# Mac 多级歌单设计

## 1. 目标

让 mac 客户端支持当前用户的多级歌单：在侧栏浏览文件夹树与歌单，完成歌单/文件夹的增删改查与组织（重命名、删除、移动、嵌套），以及歌单成员的增删。服务端能力已就绪，本设计只补齐 `core` crate 的编排层与 SwiftUI 的管理界面。

## 2. 范围

- core 暴露歌单树读取、歌单详情、歌单/文件夹 CRUD 与移动、成员增删。
- mac 主窗口侧栏新增「歌单」分区（文件夹树 + 歌单叶子），右侧详情展示歌单曲目。
- 全部组织操作走菜单驱动（工具栏菜单 + 右键菜单 + 目标选择器），不做拖放。
- 歌单曲目复用现有单曲流式试听，每行独立播放。
- **不做**：播放队列/连续播放、拖放重排与嵌套、跨用户共享歌单、智能/动态歌单、歌单封面管理、歌单内曲目重排。

## 3. 服务端（已就绪，不改动）

标准 OpenSubsonic：`getPlaylists`、`getPlaylist`、`createPlaylist`、`updatePlaylist`、`deletePlaylist`。
扩展多级树 `/rest/ext/*`：`getPlaylistTree`、`createPlaylistFolder`、`updatePlaylistFolder`、`deletePlaylistFolder`、`movePlaylist`、`moveFolder`。所有权与文件夹环检测在服务端强制。本设计不修改服务端；若测试暴露服务端缺陷，另开任务并先改 spec。

## 4. core 层

新增 `core/src/api/playlist.rs`，`MusicClient` 门面转发。全部 HTTP 编排、DTO 解码、组合逻辑在 core，Swift 不解析 HTTP、不复制业务规则。

DTO：`contract::Playlist`、`contract::PlaylistFolder` 经 `ffi_types.rs` 的 `#[uniffi::remote(Record)]` 暴露；新增 core 自有 `uniffi::Record`：

- `PlaylistTree { folders: Vec<PlaylistFolder>, playlists: Vec<Playlist> }`
- `PlaylistDetail { playlist: Playlist, tracks: Vec<Track> }`

方法与端点映射：

| core 方法 | 端点 | 说明 |
|---|---|---|
| `playlist_tree() -> PlaylistTree` | `ext/getPlaylistTree` | 一次拿全树，UI 本地组装层级 |
| `playlist_detail(id) -> PlaylistDetail` | `getPlaylist` | 含访问控制过滤后的曲目 |
| `create_playlist(name, folder_id?, song_ids) -> Playlist` | `createPlaylist` (+ `ext/movePlaylist`) | 见下方组合逻辑 |
| `rename_playlist(id, name)` | `updatePlaylist` | 只改名 |
| `set_playlist_comment(id, comment)` | `updatePlaylist` | 只改备注 |
| `add_tracks(id, song_ids)` | `updatePlaylist` | `songIdToAdd` |
| `remove_track_at(id, index)` | `updatePlaylist` | `songIndexToRemove` |
| `delete_playlist(id)` | `deletePlaylist` | |
| `move_playlist(id, folder_id?)` | `ext/movePlaylist` | `folder_id=None` 移到根 |
| `create_folder(name, parent_id?) -> PlaylistFolder` | `ext/createPlaylistFolder` | |
| `rename_folder(id, name)` | `ext/updatePlaylistFolder` | |
| `delete_folder(id)` | `ext/deletePlaylistFolder` | |
| `move_folder(id, parent_id?)` | `ext/moveFolder` | 服务端做环检测 |

**组合逻辑（在 core 内，Swift 不感知）**：`createPlaylist` 端点不接受 folderId。`create_playlist` 先调 `createPlaylist` 拿到新歌单，若 `folder_id` 非空再调一次 `movePlaylist` 落位，返回落位后的 `Playlist`（position/folder 反映最终状态）。任一步失败返回对应 `CoreError`。

## 5. mac UI

**侧栏（`LibraryView` 的 `NavigationSplitView` 左栏）**：由「纯专辑 List」改为分区列表：

1. 顶部固定入口：「曲库」「搜索」。
2. 「歌单」分区：递归渲染文件夹树（`DisclosureGroup`）+ 叶子歌单；空树显示占位提示。

选择用统一枚举 `SidebarSelection { case library, case search, case playlist(String) }`，右侧详情按选择切换。选中歌单 → `PlaylistDetailView`；选中曲库/搜索 → 现有专辑/搜索视图。

**`PlaylistViewModel`（`@MainActor ObservableObject`）**：持有 `PlaylistTree` 与当前 `PlaylistDetail`，封装全部 CRUD 调用 core，成功后局部刷新树/详情，失败置 `errorMessage`。它是唯一与 core 歌单 API 交互的地方。

**`PlaylistDetailView`**：展示选中歌单曲目列表，每行复用现有 `MediaViewModel` 单曲试听；行右键「移出歌单」（按 index 调 `remove_track_at`）；顶部可改名、编辑备注。

**菜单驱动 CRUD**：
- 侧栏工具栏「+」菜单：新建歌单、新建文件夹（弹出命名输入，可选目标文件夹）。
- 树节点右键菜单：重命名、删除、移动到…。
- 「移动到…」「加入歌单…」用弹出目标选择器，列出文件夹树（移动用）或歌单列表（加曲用）；「移到根目录」为显式选项。
- 删除文件夹/歌单前 `confirmationDialog` 二次确认；删非空文件夹提示其内歌单一并移除（与服务端行为一致）。

**曲目加入歌单**：`MediaDetailView` 专辑曲目行与 `PlaylistDetailView` 歌单曲目行的右键「加入歌单 ▸」子菜单，列出全部歌单，调 `add_tracks(id, [songId])`。

## 6. 错误处理

- core 方法失败返回 `CoreError`（网络/服务端/无效请求/未认证），Swift 映射为可读文案。
- 单个 CRUD 失败只置错误态并保留原树，不做乐观改写后回滚——操作成功后才刷新。
- 移动产生环由服务端拒绝（`ext/moveFolder`），core 透传参数错误，UI 提示"不能移动到自身或子文件夹"。
- 加入歌单/移出曲目失败提示并保留原状态。

## 7. 测试

- **core（TDD）**：`wiremock` 覆盖每个方法的请求编码与响应解码——含 `create_playlist` 带/不带 folder 的两步组合、`move_playlist(None)` 移根、空 song 列表、`remove_track_at` 的 index 传参、错误码传播。
- **XCTest**：`PlaylistViewModel` 用 mock client 覆盖 建/删/改名/移动/加曲/移曲 后的树与详情刷新、错误态；侧栏选择枚举切换。
- **端到端**：真实 Docker 服务完成 建文件夹 → 建歌单（落入文件夹）→ 从专辑加曲 → 查看曲目 → 移出曲目 → 移动歌单到根 → 删除歌单/文件夹。
- 全量执行 `cargo test`、`cargo clippy -- -D warnings`、`cargo fmt --check` 与 Swift test。

## 8. 架构边界

网络、认证、歌单 DTO 解码与 create+move 组合逻辑位于 Rust core；Swift `PlaylistViewModel` 只协调 UI 状态，SwiftUI 只渲染与转发操作。歌单 DTO 复用 `contract`，不在客户端重复定义数据模型。授权在服务端强制，客户端仅呈现其可见结果。
