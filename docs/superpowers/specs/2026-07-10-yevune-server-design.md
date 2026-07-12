# 音乐服务 · 服务端设计文档

- **日期**：2026-07-10
- **子项目**：1 / 服务端（Rust）
- **状态**：设计已确认，待 review

---

## 1. 背景与范围

搭建一个自托管的音乐流媒体服务，本质上类似 Navidrome / Plex-for-music。

**使用场景（已确认）**：个人 + **家庭**自部署产品，**不对外**，但对内是**真多用户**：

1. 首启创建管理员，管理员再为家人创建账号。
2. **每个用户有独立的歌单空间**（各自的歌单 + 文件夹树，默认私有，互不干扰）。
3. **曲库访问控制**：管理员可为曲目/专辑/艺人配置开放给哪些用户/角色，**默认全部开放**。

部署要**对非技术用户（小白）友好**。**不做**多租户 SaaS、计费、水平扩展、海量并发。

**协议契合**：OpenSubsonic 协议原生多用户（每请求带 `u=用户名`，歌单、收藏、播放计数天然按用户隔离），因此"独立歌单空间"零阻力贴合协议。真正的自研扩展只有**曲库访问控制**。

**存储**：兼容 S3 的对象存储，实际采用 **Garage**。Garage 的权威 `yevune` bucket 是原始音频唯一源（source of truth），正式原始音频键统一位于 `library/` 前缀。该 bucket 只向**单个服务端实例**授予写/删凭据；客户端不得直写。外部直传使用独立的非权威 inbox bucket/凭据，后续由受控导入流程转入权威 bucket。

**转码诉求**：音乐库以无损（FLAC/ALAC）为主，需在移动端/外网播放时**实时转成 AAC/Opus 省流量**。

### 硬约束

1. 服务端：内存占用尽可能低、性能好 → 选用 **Rust**。
2. 客户端：内存占用小、流畅度高 → **原生 UI**（排除 Electron/Flutter/RN）。
3. 接口类型尽可能全平台复用。
4. 部署对小白友好（一键起、不强制 HTTPS）。

---

## 2. 整体平台拆解与路线图

这是一个**平台**，拆成多个子项目，各自独立走 设计 → 计划 → 实现 循环。

| 顺序 | 子项目 | 依赖 | 说明 |
|---|---|---|---|
| **1** | **服务端 (Rust)** | Garage | 地基。定义数据模型 + API 契约（**本文档**） |
| **2** | **contract（Rust 共享类型）** | — | 服务端与客户端共用的 DTO，生成 OpenAPI |
| **3** | **core（Rust 客户端核心）** | contract | UniFFI 生成各语言绑定，跨平台共享逻辑 |
| **4** | **apple（iOS + macOS）** | core | 主力使用场景，优先 |
| **5** | **web（浏览器）** | contract | TS 类型 + REST 直连 |
| **6** | **android（未来）** | core | Compose 原生 UI |
| **7** | **desktop（Win/Linux，未来）** | core | Tauri 或原生，待定 |

**早期验证策略**：服务端兼容 OpenSubsonic，因此在自研客户端就绪前，可直接用现成成熟客户端（Amperfy、play:Sub 等）验证服务端——相当于免费测试客户端。

**本文档只覆盖子项目 1（服务端）。** 其余子项目各自开新的设计循环。

---

## 3. 跨平台客户端架构（背景，非本文档实现范围）

记录整体策略，确保服务端 API 设计与之契合。

**核心思想**：只有 UI 是平台专属的；其余逻辑用 Rust 写一次，编译到所有平台。

- **`contract` (Rust crate)**：纯数据类型（DTO：曲目/专辑/歌单/播放请求…）。服务端和所有客户端共用**同一份 Rust 结构体**，接口类型不可能漂移。从它生成 OpenAPI 供非 Rust 消费者（web）使用。
- **`core` (Rust crate)**：客户端共享核心——API 客户端、认证、离线缓存、播放队列/状态机、转码协商、同步逻辑。**UniFFI** 自动生成 Swift/Kotlin 绑定；Win/Linux 直接链 Rust 或走 C ABI。新增原生平台 = 只写 UI，核心零改动。
- **音频输出边界**：真正的解码送声卡、锁屏控制、后台播放留在各平台原生层（Apple AVAudioEngine 等），`core` 负责其之上的一切逻辑。
- **Web**：复用 `contract` 生成的 **TypeScript 类型** + 直接调 REST（浏览器环境与原生差异大，Rust→WASM 收益有限）。Rust WASM 核心为未来可选升级。

**成熟度佐证**：此架构在生产广泛验证——Matrix/Element X（matrix-rust-sdk + UniFFI）、Mozilla/Firefox（UniFFI 创造者）、Signal、1Password、Bitwarden、Dropbox 同步引擎。

---

## 4. Monorepo 布局

单 monorepo，物理放一起，各子项目用各自工具链（Cargo / SwiftPM+Xcode / npm）。核心动机：**API 契约在服务端与客户端间严格同步**，改接口一次原子提交。

```
Yevune/
├── server/          # 【1】Rust 服务端 (axum)
│   ├── Cargo.toml   # Rust workspace 根
│   └── src/         # 先用模块划分，长大再提升为独立 crate（YAGNI）
│       ├── api/         # OpenSubsonic + 扩展接口
│       ├── index/       # SQLite 元数据索引
│       ├── storage/     # Garage/S3 客户端
│       ├── scanner/     # 入库 + 读标签
│       └── transcode/   # FFmpeg 转码管线
│
├── contract/        # 【2】Rust 共享类型 → 服务端+客户端共用，生成 OpenAPI
├── core/            # 【3】Rust 客户端核心 → UniFFI 生成各语言绑定
├── clients/
│   ├── apple/       # 【4】SwiftUI (iOS + macOS)
│   ├── web/         # 【5】浏览器 (TS + 轻量框架)
│   ├── android/     # 【6】Compose (未来)
│   └── desktop/     # 【7】Win/Linux (未来)
│
├── docs/superpowers/specs/   # 各子项目设计文档
└── README.md
```

---

## 5. 服务端架构与技术选型

单体 Rust 二进制，内部按职责分模块：

| 模块 | 职责 | 选型 |
|---|---|---|
| **HTTP API 层** | OpenSubsonic + 扩展接口 | `axum`（tokio 生态，开销低） |
| **元数据索引库** | 曲目/专辑/艺人/歌单/标注 | **SQLite** via `sqlx`（嵌入式、零运维、省内存） |
| **对象存储客户端** | 读写 Garage | `object_store` 或 `aws-sdk-s3` |
| **扫描/入库器** | 发现文件、读嵌入标签建库 | `symphonia`/`lofty` 纯 Rust 读标签 |
| **转码管线** | FLAC/ALAC → AAC/Opus，按需 + 缓存 | FFmpeg 子进程 |
| **认证** | 家庭多用户 + 小白友好 | OpenSubsonic token/明文 + 自研 Bearer |

**核心数据流**：上传 FLAC 到 Garage → 扫描器读标签写入 SQLite 索引 → 客户端请求播放 → 命中转码缓存则直接流，否则 FFmpeg 实时转码并缓存回 Garage → HTTP Range 分发。

### 存储位置决策

- **SQLite 索引** → 服务器**本地磁盘**（数据库跑对象存储上性能极差）。
- **转码缓存** → **Garage**（体积大、可重建、适合对象存储）。

### 数据存储选型决策（SQLite，无 Redis / 无 Postgres）

个人/家庭（少量用户）场景，明确**不引入 Postgres、不引入 Redis**：

- **SQLite 不慢**：进程内嵌入式，无网络/IPC 往返，读密集场景常快于 Postgres（参照 Navidrome 扛 10 万+ 曲库）。短板是并发写，但本场景写负载极小（偶尔扫描/改标签/scrobble），开 **WAL 模式**足矣。
- **Postgres/Redis 会拉起独立容器、常驻内存**，直接顶撞"省内存 + 小白友好"两条硬约束，对家庭少量用户零收益，属过早优化。Redis 能干的活儿（缓存、会话、任务队列）分别被 OS 页缓存 / SQLite / 进程内 tokio 任务覆盖。
- **留后路**：数据访问层用 `sqlx`（同时支持 SQLite/Postgres）。若将来扩到多用户公开产品，再迁 Postgres 可行——"到时候再说"。

**docker-compose 内容**：仅 `服务端 + Garage + FFmpeg` 三件。

---

## 6. 数据模型（SQLite）

对齐 OpenSubsonic 语义。

| 表 | 关键字段 | 说明 |
|---|---|---|
| **users** | id, name, password_enc, created_at | 真多用户；密码可逆加密存储（见 §10） |
| **roles** | id, name, is_builtin | 内建 `admin`/`member` + 管理员自建角色（如"孩子""大人"） |
| **user_roles** | user_id, role_id | 用户 ↔ 角色 多对多 |
| **artists** | id, name, sort_name, mbid?, cover_key? | 艺人 |
| **albums** | id, name, artist_id, year, genre, cover_key, added_at | 专辑 |
| **tracks** | id, title, album_id, artist_id, disc_no, track_no, duration, codec, bitrate, size, **object_key**, **etag**, content_hash, replaygain, added_at | 曲目；`object_key` 指向 Garage 原始文件 |
| **annotations** | user_id, item_type, item_id, starred_at, play_count, last_played, rating | 收藏/播放次数/评分，按用户隔离 |
| **tag_overrides** | track_id, field, value | 改标签的覆盖层（见 §9），不动原文件 |
| **playlist_folders** | id, **owner_id**, name, **parent_id**(自引用,可空), position | 每用户一棵文件夹树；parent_id 空 = 顶级 |
| **playlists** | id, **owner_id**, name, comment, **folder_id**(可空), position | 歌单叶子，默认私有；folder_id 空 = 根级 |
| **playlist_tracks** | playlist_id, track_id, position | 歌单内曲目有序 |
| **access_rules** | id, **scope_type**(track/album/artist/genre), scope_id, created_by, created_at | 曲库访问控制规则（见下），仅为被限制内容存行 |
| **access_rule_grants** | rule_id, principal_type(user/role), principal_id | 规则的允许名单 |
| **transcode_cache** | track_id, format, bitrate, object_key, size, created_at, last_access | 转码产物登记（文件本体在 Garage） |
| **scan_state** | last_scan_at, cursor | 扫描进度/断点 |

**多级歌单模型**：文件夹只做容器（本身不装曲目），歌单是装曲目的叶子。任意深度嵌套。例：`中文`(folder) → `精选`/`流行`/`摇滚`(playlist)，或 `中文` → `华语经典`(subfolder) → …。**每个用户拥有各自独立的歌单树**（`owner_id` 隔离，默认私有）。

**曲库访问控制模型**：
- **默认开放**：一首歌若无任何限制规则 → 所有用户可见。只为被限制的内容存 `access_rules` 行（默认零成本、省空间）。
- **规则 = 作用域 + 允许名单**：一条规则指定作用域（曲目/专辑/艺人/流派）和允许访问的用户/角色（`access_rule_grants` 允许名单）。
- **多级作用域，最具体优先**：曲目规则 > 专辑规则 > 艺人规则 > 流派规则。可"限制整个艺人"再"单独开放某专辑"。
- **查询时评估**（非逐曲固化）：扫描新入库的曲目**自动继承**其所在专辑/艺人的规则，无需重新配置。
- **管理员永远可见全部**并负责配置；浏览/搜索/播放的每个查询按当前用户过滤可见性。
- 允许名单语义（"仅这些人可见"）覆盖家庭场景；"除某角色外都可见"通过给其余角色授权实现。deny 名单作为未来可选。

**变更检测**：`tracks` 存每个文件的 Garage **ETag + size**；扫描时列举 bucket 对比 ETag → 判定新增/修改/删除，增量处理。

**搜索**：SQLite **FTS5** 虚拟表覆盖曲目/专辑/艺人名，支撑 `search3`。

**封面图**：入库时从音频抽取内嵌封面单独上传 Garage，`cover_key` 引用，避免播放时重复解析。

---

## 7. 入库 / 扫描（双路径）

Garage 是唯一源，且其 bucket 事件通知能力有限，故采用**主动扫描** + **客户端管理 API** 双路径，共用同一套入库逻辑（读标签 → 抽封面 → 写索引）。

**单写者边界**（见 ADR-0006）：Garage v2.3 的 DeleteObject 是无条件删除，不能用 ETag 做原子 compare-and-delete。权威 `yevune` bucket 因而只允许一个服务端实例写/删，服务端用共享逐键锁 + SQLite CAS 串行化正式键变更。多实例部署前必须先引入跨实例协调，不能直接复用当前写路径。外部直传只能进入独立 inbox bucket；inbox 是非权威暂存，当前阶段不提供 inbox 消费接口。

| 路径 | 用途 | 时机 |
|---|---|---|
| **主动扫描 Garage**（自定义/手动触发、可配范围前缀） | 批量更新、补漏、纠偏 | 全程保留作兜底 |
| **客户端音频文件管理 API**（上传/改标签/删除/整理 → 写权威 Garage 后即时入库） | 后期主力；`uploadTrack`/`moveTrack` 仅接受 `library/...` 正式键 |

**扫描流程**（增量、省资源）：
1. 列举 bucket 对象（分页）。
2. 每个音频对象用 `(key, etag, size)` 比对 `tracks`：
   - 新增 → **Range 读取文件头部**（FLAC 元数据块/ID3 在开头）解析标签，无需下整个文件 → 抽内嵌封面单独上传 → 写 track/album/artist。
   - etag 变化 → 重读标签更新。
   - DB 有 bucket 无 → 标记删除。
3. 更新 `scan_state`（断点续扫）。
4. tokio 并发限流。

**触发**：手动（`startScan`）+ 定时（可配间隔）+ 上传接口即时入库。

---

## 8. 转码管线（按需 + 缓存，不预转码）

客户端请求 `/stream?id=X&format=aac&maxBitRate=192` 时：

1. **判断是否转码**：客户端要原格式、或原文件已符合目标且兼容 → **直接透传**（Garage Range GET，最省 CPU）。否则转码。
2. **查缓存** `transcode_cache(track_id, format, bitrate)`：
   - 命中 → 从 Garage 流式返回缓存对象（可 Range、可 seek）。
   - 未命中 → FFmpeg 实时转码。
3. **实时转码机制**：FFmpeg 子进程，输入来自 Garage 流，输出目标编码到 stdout → 一边发客户端、一边落临时文件；成功后上传 Garage + 写缓存登记。**客户端中途断开则丢弃半成品，绝不缓存不完整文件。**
4. **并发限流**：信号量限制同时运行的 FFmpeg 进程数，给 CPU/内存封顶。

**缓存键**：`transcode/{track_id}/{format}_{bitrate}.{ext}`，存 Garage。因可重建，可安全淘汰（可选配置最大体积 + LRU by last_access）。

**分发**：
- 直传/命中缓存 → HTTP Range，完整 seek。
- 首次实时转码 → 分块流式（大小未知，seek 受限）；落缓存后即可 seek。
- **HLS 自适应**（`hls.m3u8`）→ 后续增强，v1 先跑通直传 + 实时转码 + 缓存。

**决策**：按需 + 缓存，**不预转码**（省存储、省无用功，首播稍有转码延迟，个人场景可接受）。

---

## 9. API 接口面

两层：**OpenSubsonic 兼容层**（现成客户端可用）+ **自研扩展层**（原生客户端专享）。扩展通过 `getOpenSubsonicExtensions` 声明，智能客户端可发现。

### OpenSubsonic 兼容子集（v1 必备）

| 类别 | 接口 |
|---|---|
| 系统 | `ping`, `getLicense`, `getOpenSubsonicExtensions` |
| 浏览 | `getArtists`, `getArtist`, `getAlbum`, `getSong`, `getAlbumList2`, `getGenres`, `getIndexes` |
| 搜索 | `search3`（走 FTS5） |
| 歌单 | `getPlaylists`, `getPlaylist`, `createPlaylist`, `updatePlaylist`, `deletePlaylist` |
| 媒体 | `stream`, `download`, `getCoverArt`（`hls.m3u8` 后置） |
| 标注 | `star`, `unstar`, `setRating`, `scrobble` |
| 扫描 | `getScanStatus`, `startScan` |
| 用户 | `getUser`, `getUsers`, `createUser`, `updateUser`, `deleteUser`, `changePassword` |

`getPlaylists` 返回**当前用户可见歌单的扁平列表** → 现成客户端照常可用（只是看不到层级）。浏览/搜索/媒体接口均按当前用户的曲库访问控制过滤。

### 自研扩展层（命名空间隔离，如 `/rest/ext/*`）

| 类别 | 接口 | 说明 |
|---|---|---|
| **多级歌单** | `getPlaylistTree`, `create/update/deletePlaylistFolder`, `movePlaylist`, `moveFolder` | 文件夹树 + 歌单叶子 |
| **库管理（写）** | `uploadTrack`(multipart→Garage+入库), `updateTags`, `deleteTrack`, `moveTrack` | 客户端音频文件管理；upload/move 仅接受非空 `library/...` 正式键 |
| **访问控制** | `setAccessRule`(scope+允许名单), `getAccessRules`, `deleteAccessRule` | 管理员配置曲库开放范围，默认开放 |
| **角色管理** | `getRoles`, `createRole`, `deleteRole`, `assignRole`, `unassignRole` | 内建 admin/member + 自定义角色 |
| **扫描增强** | 自定义范围/前缀扫描触发 | 补漏、纠偏 |

访问控制与角色管理接口仅管理员可调用。

**`updateTags` 处理策略（决策 A：覆盖层）**：改动存 `tag_overrides` 覆盖层，**不动 Garage 原文件**（快、非破坏性、不变 etag）。展示时以覆盖层优先于文件标签。另提供显式"写回文件"操作（下载→改标签→重传）供需要时使用。

---

## 10. 认证

- **OpenSubsonic 认证**（`u`/`t`/`s` token，或明文密码）→ 兼容现成客户端**必须支持**。
- **不强制 HTTPS**：默认支持明文 HTTP，方便小白局域网/本机一键起服务。纯 HTTP 下优先用 token 认证（md5(密码+盐)）避免裸传密码。TLS/反代作为进阶可选。
- **密码存储**：Subsonic token 认证要求服务端能还原密码校验，故密码**可逆加密存储**（同 Navidrome 做法），个人 + HTTP 场景可接受。
- **自研客户端**：额外发 **Bearer token**（会话令牌）走扩展接口，比 Subsonic 老式认证更干净。
- **多用户与权限**：首启创建管理员；管理员通过用户/角色管理接口创建家人账号并分配角色。每个请求解析出用户身份 → 决定其曲库可见性、歌单空间、管理员专属接口的授权。**授权（谁能看什么、谁是管理员）在服务端强制**，客户端不可绕过。
- 家庭场景无需 OAuth 等复杂机制。

---

## 11. 配置、部署与可观测性

### 部署（小白友好为核心）

- **单个静态二进制**（musl 编译，跨发行版可移植）。唯一外部依赖 FFmpeg。
- **一键起**：**docker-compose** 打包 `服务端 + Garage + FFmpeg`，`docker compose up` 一条命令全起。
- **单一 TOML 配置** + 环境变量覆盖，全部带合理默认：监听地址/端口、Garage 端点/bucket/凭证、SQLite 路径、转码缓存上限、扫描间隔、默认转码格式/码率、FFmpeg 路径、可选 TLS 证书。
- **首启引导**：无用户时通过配置或简单 Web 设置页创建管理员。
- **默认明文 HTTP**，TLS/反代进阶可选。

### 可观测性（轻量）

- 结构化日志 `tracing`，级别可配。
- 健康检查：`ping` + `/healthz`。
- 扫描/转码状态可查（`getScanStatus`）。
- Prometheus `/metrics` → 可选，v1 不做（YAGNI）。

### 资源守护（呼应省内存）

- 有界并发：转码信号量、扫描并发上限。
- 流式有界缓冲：转码/传输绝不把整个文件读进内存。
- 连接数限制。

---

## 12. 明确排除（YAGNI / 未来）

- 多租户、计费、水平扩展、海量并发（非本场景）。
- Postgres、Redis（家庭少量用户过早优化，留 `sqlx` 后路）。
- HLS 自适应转码（v1 后置增强）。
- Prometheus 指标（可选，后置）。
- 预转码（采用按需 + 缓存替代）。
- Web 端复用 Rust WASM 核心（先用 TS 类型 + REST，未来可选）。
- 客户端（apple/web/android/desktop）各自独立子项目，本文档不含。

---

## 13. 后续步骤

本文档确认后 → 进入服务端子项目的**实现计划**（writing-plans）。其余子项目（contract / core / 各客户端）待服务端就绪后各自开新的设计循环。
