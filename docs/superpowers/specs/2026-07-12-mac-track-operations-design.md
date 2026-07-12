# Mac 曲目操作中心设计（并行任务 A）

## 1. 目标

把已实现但**未接线**的曲目管理能力接进曲库界面，并做操作优化与多选批量。当前 `TagEditorView` + `TagEditorViewModel`（标签编辑/删除/移动）已完整实现，但没有任何界面构造或弹出它；`MediaDetailView` 曲目行右键只有「加入歌单」。本任务补齐入口、打磨交互、增加多选批量。

## 2. 范围与并行边界

- **独占文件**：`clients/apple/Sources/MusicApp/Views/MediaDetailView.swift`、`clients/apple/Sources/MusicApp/Views/TagEditorView.swift`、`clients/apple/Sources/MusicApp/Model/TagEditorViewModel.swift`，以及本任务新建的文件与测试。
- **纯 Swift**：core 已有 `update_tags`/`delete_track`/`move_track`；曲目对象键已由共享 prep 暴露为 `Track.path`（`contract::Track`）。**本任务不改 core、不改 `contract`、不改 server。**
- **不得改动**（归并行任务 B 或公共）：`LibraryView.swift`、`LibraryViewModel.swift`、`LoginViewModel.swift`（`MusicClientProviding` 协议）、`CoreMusicClient.swift`、`PlaylistViewModel.swift`、`core/*`。
- **关键约束**：**保持 `MediaDetailView` 的初始化签名不变**（当前 `init(album:model:playlists:)`），任务 B 会改它的调用点，签名一变即冲突。曲目管理所需的客户端访问一律经 `MediaViewModel`（它已持有 `client` 与专辑详情，并有 `load(album:)` 刷新）路由——例如给 `MediaViewModel` 加一个 `makeTagEditor(for:) -> TagEditorViewModel` 工厂或直接加 `updateTags/deleteTrack/moveTrack/reload` 方法，避免把 `client` 塞进 `MediaDetailView` 的构造参数。
- **不做**：批量移动对象键（移动仅单条）、专辑/艺人级操作、撤销/回收站。

## 3. 界面与交互

**曲目行右键菜单**（在现有「加入歌单」基础上补齐）：
- 「编辑标签…」→ 弹 sheet 呈现 `TagEditorView`。
- 「移动…」→ 可并入标签 sheet 的「整理」区（`TagEditorView` 已有 moveKey/移动/删除区），或单独条目。
- 「删除」→ `confirmationDialog` 二次确认后调用删除。

**标签编辑（`TagEditorViewModel` 打磨）**：
- 现状缺陷：字段初值全空、未预填当前曲目值；`didSave` 无人观察；移动键要手输。
- 改为：新增以当前 `Track` 预填的初始化（标题/专辑/艺人/流派/年份/曲序/碟序），`moveKey` 预填 `track.path`。
- 保存成功（`didSave`）后关闭 sheet 并触发 `MediaViewModel.load(album:)` 刷新专辑详情；失败在 sheet 内显示 `errorMessage`。
- 空字段表示「保持原值」（`TagUpdate` 相应字段传 `None`，沿用现有 `value(_)` 逻辑）。

**删除 / 移动**：删除前二次确认；删除或移动成功后刷新专辑详情；操作错误统一经 `errorMessage` 呈现。

**多选批量**：
- 曲目 `List` 支持多选（`selection: Set<String>` 绑定曲目 id）。
- 选中≥1 条时显示批量操作栏：批量删除（二次确认，逐条 `delete_track`）、批量加入歌单（复用 `PlaylistViewModel.addTracks`，一次传多个 id）、批量改标签（仅填写的共同字段应用到每条，逐条 `update_tags`）。
- 批量为客户端逐条有界调用（无批量 core 接口）；单条失败不中断，末尾汇总失败数并刷新。

## 4. core

无改动。复用 `MusicClient` 的 `update_tags(id, TagUpdate)`、`delete_track(id)`、`move_track(id, key)`；`Track.path` 已由 prep 提供。

## 5. 错误处理

- 所有失败经可读文案呈现（sheet 内或详情区），不静默吞掉。
- 批量操作逐条独立，部分失败保留成功项并提示失败计数。
- 移动到非法/冲突键由服务端拒绝并回传，UI 呈现。

## 6. 测试

- **XCTest**：`TagEditorViewModel` 用当前 Track 预填后各字段正确、`moveKey` 预填 `path`；保存只传非空字段、`didSave` 置位；删除/移动调用正确参数（用 mock client 记录调用）。批量逻辑（批量删除逐条、批量加标签只应用共同字段、批量加入歌单一次多 id）用 mock 覆盖。
- 视图交互（右键菜单、多选栏、sheet 弹出）以 `swift build` 编译 + 手动冒烟为准。
- 全量 `swift test` 通过。

## 7. 架构边界

Swift 只协调 UI 与转发操作，网络/DTO 在 Rust core。曲目管理经 `MediaViewModel` 路由客户端调用，`MediaDetailView` 构造签名保持稳定以维持与并行任务 B 的免冲突。
