# Mac 管理客户端设计文档

- **日期**：2026-07-11
- **子项目**：Mac 客户端（含 `core` 客户端核心的第一批切片）
- **里程碑**：M1 — 曲库内容管理 + 封面 + 试听
- **状态**：设计已确认，待实现（交 Codex）

---

## 1. 背景与范围

服务端子项目已完成（OpenSubsonic 兼容 + 自研扩展 + 家庭多用户 + 访问控制 + docker-compose 部署）。现在开始客户端，**从 Mac 起步**。

**Mac 的产品定位**：**管理型桌面客户端**——把曲库建起来、整理好。iOS（后续）作消费型客户端。桌面是管理曲库的天然场所：先有内容、整理好，播放才有意义。

**M1 范围（本文档）**：
- **A 内容管理**：登录 → 浏览曲库 → 上传曲目 → 改标签 → 删除 → 移动/整理 → 触发并监控扫描。
- **B 封面 + 试听**：封面显示与替换；管理时轻量试听（验证文件传对没）。

**M1 排除（留后续迭代）**：管理员功能（用户/角色/访问控制规则 UI）、离线下载、iOS target。

**遵守**：[`AGENTS.md`](../../../AGENTS.md) 全部红线与工作流；[ADR-0001](../../adr/0001-rust-core-native-ui.md)（Rust 核心 + 原生 UI + UniFFI）。

---

## 2. 架构

**只有 UI 是平台专属；逻辑全在 `core`。**

```
contract (Rust DTO, 已存在)
   └── core (Rust, 依赖 contract) ──UniFFI──> Swift 绑定 (xcframework)
                                                     └── clients/apple (SwiftUI, macOS)
音频出声：clients/apple 内 AVFoundation（core 只做取流/URL，不出声）
```

- **`core`**：服务器连接、认证、API 客户端、管理操作、流 URL 生成。写一次，未来 iOS/Android 复用。
- **`clients/apple`**：SwiftUI 视图 + 视图模型（调用 core）+ 原生预览播放器。
- **边界**：core 不碰 UI、不出声；App 不实现网络/协议逻辑，只调 core。

---

## 3. `core` 模块划分

| 文件 | 职责 |
|---|---|
| `core/src/config.rs` | 服务器连接配置（地址、凭证载体） |
| `core/src/auth.rs` | 登录，签发/持有会话 Bearer + OpenSubsonic 认证参数 |
| `core/src/http.rs` | 认证 HTTP 客户端（reqwest），统一错误映射 |
| `core/src/api/browse.rs` | getArtists/getArtist/getAlbum/getSong/getAlbumList2/getGenres/getIndexes/search3 |
| `core/src/api/manage.rs` | uploadTrack(流式)/updateTags/deleteTrack/moveTrack |
| `core/src/api/scan.rs` | startScan/getScanStatus + 范围扫描 |
| `core/src/api/media.rs` | coverArtUrl/streamUrl 生成 |
| `core/src/client.rs` | UniFFI 门面 `MusicClient`，聚合上述能力 |
| `core/src/error.rs` | 类型化错误（网络/认证/服务端错误码），暴露给 Swift |

DTO 直接复用 `contract` crate 的类型，经 UniFFI 暴露给 Swift；不重复定义。

---

## 4. UniFFI 边界

门面对象 `MusicClient`（async 方法，UniFFI 支持 async）：

| 方法 | 说明 |
|---|---|
| `login(server, user, password) -> Session` | 建立会话，后续调用自动带认证 |
| `list_albums(sort, offset, size) -> Vec<Album>` / `get_album(id)` / `get_artist(id)` | 浏览 |
| `search(query) -> SearchResult` | 搜索（search3） |
| `upload_track(local_path, meta) -> Track` | **传本地文件路径**，core 流式读→multipart→服务端；不把整文件塞过 FFI |
| `update_tags(track_id, tags)` | 覆盖层改标签 |
| `delete_track(id)` / `move_track(...)` | 删除/移动 |
| `start_scan(scope?)` / `scan_status() -> ScanStatus` | 扫描触发/轮询 |
| `cover_art_url(id) -> String` / `stream_url(track_id, params) -> String` | 供 SwiftUI AsyncImage / AVPlayer 用 |

**进度上报**：上传进度用 UniFFI callback interface（`UploadProgress`），扫描进度用轮询 `scan_status()`。
**资源守护**：上传流式、有界缓冲（呼应 AGENTS.md 红线）。

---

## 5. Mac 应用结构（SwiftUI, macOS 14+）

```
clients/apple/
├── Packages/YevuneCoreFFI/        # UniFFI 生成的 Swift 绑定 + xcframework 封装
├── Sources/Yevune/
│   ├── App.swift            # 入口，注入 MusicClient
│   ├── Views/
│   │   ├── LibraryView.swift        # 艺人/专辑/曲目浏览 + 搜索栏
│   │   ├── AlbumDetailView.swift    # 专辑详情 + 曲目列表 + 试听按钮
│   │   ├── UploadView.swift         # 拖拽上传区 + 进度
│   │   ├── TagEditorView.swift      # 改标签表单
│   │   ├── ScanStatusView.swift     # 扫描触发 + 状态
│   │   └── CoverView.swift          # 封面显示 + 替换
│   ├── Audio/PreviewPlayer.swift    # AVFoundation 试听（消费 stream_url）
│   └── Model/*ViewModel.swift       # ObservableObject，包裹 MusicClient async 调用
└── (Xcode 工程或 SwiftPM 可执行 target)
```

视图模型是 UI 与 core 的唯一桥：视图不直接持有网络状态，只观察视图模型。

---

## 6. 各功能数据流

- **登录**：`LoginView` → `client.login` → 会话入内存 → 进入主界面。
- **浏览**：`LibraryVM` → `client.list_albums/get_album/search` → `contract` DTO → SwiftUI 列表/网格。
- **上传**：拖入文件 → `UploadVM` 取本地路径 → `client.upload_track`（流式，回调进度）→ 服务端写 Garage + 即时入库 → 刷新列表。
- **改标签**：`TagEditorView` 提交 → `client.update_tags`（覆盖层，不动原文件）→ 刷新。
- **删除/移动**：`client.delete_track/move_track` → 刷新。
- **扫描**：`ScanStatusView` 触发 `client.start_scan` → 轮询 `client.scan_status` → 展示进度。
- **封面**：显示走 `client.cover_art_url` + AsyncImage；替换 → 选本地图 → 上传关联（见 §9 服务端依赖）。
- **试听**：`AlbumDetailView` 点播 → `client.stream_url` → `PreviewPlayer` 用 AVPlayer 播放。

---

## 7. 依赖

| 依赖 | 用途 | 说明 |
|---|---|---|
| `reqwest` | core 的 HTTP 客户端 | 客户端需 HTTP，reqwest 为 Rust 标准选择 |
| `uniffi` | 生成 Swift 绑定 | ADR-0001 已锁定 |
| `tokio` | core 异步运行时 | async reqwest 需要运行时；复用服务端已有依赖 |
| `serde` / `serde_json` | OpenSubsonic JSON 信封解析 | 不能安全地手写 JSON 解析；复用 `contract`/服务端已有依赖 |
| `AVFoundation` | macOS 原生音频 | 系统框架 |

其余能不加就不加（YAGNI）。新增依赖在提交说明给理由。

---

## 8. 构建管线（M1 最大技术风险，切片 0 优先消灭）

1. `cargo build` core，目标 `aarch64-apple-darwin`（如需 Intel 再加 `x86_64`）。
2. `uniffi-bindgen` 生成 Swift 绑定。
3. 打包为 `xcframework`。
4. SwiftPM binary target（`Packages/YevuneCoreFFI`）消费，App 依赖它。
5. 用 `cargo-swift` 或仓库内 `xtask`/脚本一键化；README 记录步骤。

**切片 0 的唯一目标**：用最小的"登录 + ping"闭环把这条链路打通，尽早暴露集成问题。

---

## 9. 服务端依赖（一处）

**替换封面**若服务端无对应端点，按现有 `/rest/ext/*` 模式补一个 `setCoverArt`（上传图片 → 关联到专辑/曲目 → 存 Garage、更新 cover_key），TDD，并在 `getOpenSubsonicExtensions` 声明。同仓库，可在本里程碑内一并完成。封面**显示**无需新端点（用现有 getCoverArt）。

---

## 10. 里程碑切片（垂直，逐片 TDD + 提交）

| 切片 | 内容 | 交付 |
|---|---|---|
| **S0** | 打通 Rust→UniFFI→xcframework→SwiftUI；登录 + ping | App 能连服务端登录成功 |
| **S1** | 浏览曲库（艺人/专辑/曲目）+ 搜索 | 能看到并搜索曲库 |
| **S2** | 拖拽上传（流式 + 进度） | 能把本地文件传进库 |
| **S3** | 改标签（覆盖层编辑器） | 能编辑元数据 |
| **S4** | 删除 / 移动 | 能整理曲库 |
| **S5** | 扫描触发 + 状态监控 | 能手动扫库补漏并看进度 |
| **S6** | 封面显示 + 替换（含 §9 服务端端点如需） | 能看/换封面 |
| **S7** | 试听（AVFoundation） | 管理时能点播验证 |

每片：core 只加该片所需逻辑 + Swift 绑定 + SwiftUI；先写失败测试→实现→跑绿→提交。

---

## 11. 测试策略

- **core**：Rust 单元/集成测试；对管理端点用本地起的服务端（docker-compose）或 mock 做集成测试。TDD 强制。
- **Swift**：XCTest 覆盖视图模型（用假 `MusicClient` 或协议抽象）；关键视图冒烟测试。
- **端到端**：`docker compose up` 起真实服务端，跑通"上传→扫描→浏览→试听"闭环。

---

## 12. 后续（非本里程碑）

- M2：管理员 UI（用户/角色/访问控制规则）。
- M3：离线下载。
- 之后：iOS target（复用同一 `core`，仅重写 UI）。
