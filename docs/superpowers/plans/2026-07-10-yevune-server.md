# 音乐服务端 实现计划（任务分解 + 交接提示词）

> **本计划的用法**：服务端拆成 11 个任务，按"波次"组织。每个任务附一份**自包含交接提示词**，可直接复制交给不同的 AI 工具（Claude Code / Codex / Cursor 等）另起会话执行。所有任务共同遵守 [`AGENTS.md`](../../../AGENTS.md) 与设计文档 [`specs/2026-07-10-yevune-server-design.md`](../specs/2026-07-10-yevune-server-design.md)。

**Goal**：交付一个 Rust 音乐流媒体服务端——从 Garage 拉取音频、建索引、按需转码、OpenSubsonic 兼容 + 自研扩展、家庭多用户 + 曲库访问控制、docker-compose 一键部署。

**Architecture**：单体 axum 二进制，模块化（api/index/storage/scanner/transcode/auth）。SQLite 本地索引，Garage 存原文件与转码缓存。契约类型在 `contract` crate 供全平台复用。

**Tech Stack**：Rust, axum, sqlx+SQLite(WAL/FTS5), object_store 或 aws-sdk-s3, symphonia/lofty, FFmpeg, tracing, UniFFI（后续客户端用）。

## Global Constraints（每个任务隐含包含，逐条摘自 spec/AGENTS.md）

- Rust；数据库仅 SQLite（`sqlx`）；**禁止 Postgres/Redis**。
- Garage 唯一源；**SQLite 本地磁盘**；转码缓存入 Garage。
- OpenSubsonic 兼容不可破坏；扩展走 `/rest/ext/*` 并在 `getOpenSubsonicExtensions` 声明。
- 转码按需 + 缓存，**绝不缓存不完整产物**；流式有界缓冲，**不把整文件读进内存**；并发用信号量限流。
- 家庭多用户；曲库默认开放，管理员按 曲目/专辑/艺人/流派 限制；**授权服务端强制**。
- 不强制 HTTPS（默认明文 HTTP）。
- **强制 TDD**：先写失败测试→跑红→最小实现→跑绿→提交；小步 Conventional Commits；`cargo test`/`clippy -D warnings`/`fmt --check` 必须全绿。

## altitude 说明

本计划在**任务粒度**分解（含范围边界、接口契约、DoD、交接提示词），因为要交给多个不同 AI 工具并行执行。**每份提示词都要求执行方按 TDD 的 bite-sized 步骤（写失败测试→跑红→实现→跑绿→提交）推进**，具体步骤由执行 agent 在其会话内产出——这是 AGENTS.md 强制的。

---

## 依赖图与执行波次

```
波次 A (1 个 agent，串行地基):   T0 → T1 → T2
波次 B (2 个 agent 并行):        T3 storage   ‖  T6 auth/users
波次 C (2 个 agent 并行):        T4 scanner   ‖  T5 transcode
波次 D (1 个 agent，集成):       T7 OpenSubsonic API
波次 E (2 个 agent 并行):        T8 扩展接口  ‖  T9 访问控制强制
波次 F (1 个 agent，收尾):       T10 OpenAPI + 部署
```

| 任务 | 依赖 | 可并行搭档 |
|---|---|---|
| T0 脚手架/治理 | — | — |
| T1 contract 类型 | T0 | — |
| T2 index/SQLite | T0, T1 | — |
| T3 storage | T1, T2 | T6 |
| T6 auth/用户/角色 | T1, T2 | T3 |
| T4 scanner | T2, T3 | T5 |
| T5 transcode | T2, T3 | T4 |
| T7 OpenSubsonic API | T2,T3,T4,T5,T6 | — |
| T8 扩展接口 | T7, T6, T2 | T9 |
| T9 访问控制强制 | T7, T6, T2 | T8 |
| T10 OpenAPI+部署 | T7, T1 | — |

**跨任务并行的协调规则**（写进各提示词）：并行任务只碰各自模块目录；共享文件（如 `server/src/api/mod.rs` 路由注册）由后集成的任务负责，前置任务只暴露 `pub fn` 供注册，不直接改路由树。

---

## 交接提示词通用前缀

> 每份提示词都以这段开头（复制时保留）：
>
> ```
> 你在为一个自托管音乐服务的 Rust 服务端工作。开始前必须：
> 1. 读完仓库根 AGENTS.md（项目宪法，含红线与强制工作流）。
> 2. 读设计文档 docs/superpowers/specs/2026-07-10-yevune-server-design.md。
> 3. 读本任务在 docs/superpowers/plans/2026-07-10-yevune-server.md 中的条目。
> 强制 TDD：每个行为先写失败测试→跑红→最小实现→跑绿→提交（Conventional Commits）。
> 只做本任务范围内的事；越界问题记 TODO 不擅自扩大。
> 完成前 cargo test / cargo clippy -- -D warnings / cargo fmt --check 必须全绿。
> 不确定就停下来问，不要臆测发挥或偏离 spec。
> ```

---

## Task 0：仓库脚手架与治理骨架

**依赖**：无（第一个）
**Files**：`server/Cargo.toml`（workspace）、`server/src/main.rs`、`server/src/config.rs`、`server/src/lib.rs`、`docker-compose.yml`（server+garage 骨架）、`.github/workflows/ci.yml`、`server/tests/health_test.rs`
**Interfaces · Produces**：`Config`（TOML+env 解析，字段见 spec §11）；可启动的 axum app，暴露 `GET /healthz`→200 与 OpenSubsonic `GET /rest/ping`→标准 ok 响应；`tracing` 初始化。

**范围**：Cargo workspace + server crate 骨架；配置加载（TOML 文件 + 环境变量覆盖，带默认值）；`tracing` 结构化日志；`/healthz` 与 `/rest/ping`；CI（fmt/clippy/test）；docker-compose 起 server+garage（FFmpeg 稍后）。**不做**任何业务逻辑。

**DoD**：`cargo test` 绿；`cargo run` 起服务，`curl /healthz` 与 `/rest/ping` 通过；CI 配置就绪；改动符合 AGENTS.md。

<details><summary>交接提示词</summary>

```
[通用前缀]
任务：T0 仓库脚手架与治理骨架。
建立 server/ 的 Cargo workspace 与 crate 骨架，实现：
- Config：从 TOML 文件 + 环境变量覆盖加载，字段含监听地址/端口、Garage 端点/bucket/凭证、SQLite 路径、转码缓存上限、扫描间隔、默认转码格式/码率、FFmpeg 路径、可选 TLS 证书，全部带合理默认。
- axum 应用骨架，暴露 GET /healthz(200) 与 OpenSubsonic GET /rest/ping（返回标准 subsonic-response ok，支持 f=json）。
- tracing 结构化日志，级别可配。
- .github/workflows/ci.yml：cargo fmt --check、clippy -D warnings、test。
- docker-compose.yml：server + garage 两服务骨架（FFmpeg 后续任务加）。
先为 /healthz 和 /rest/ping 写失败集成测试，再实现。不引入任何数据库/业务逻辑。
DoD：cargo test 绿，服务可启动并响应两个端点，CI 就绪。
```
</details>

---

## Task 1：`contract` crate — 共享 DTO 类型

**依赖**：T0
**Files**：`contract/Cargo.toml`、`contract/src/lib.rs`、`contract/src/{media,playlist,user,access,stream,error}.rs`、`contract/tests/serde_test.rs`
**Interfaces · Produces**：`Artist, Album, Track, Genre`；`Playlist, PlaylistFolder`；`User, Role`；`AccessRule{scope_type, scope_id}, Principal`；`StreamRequest{id, format, max_bitrate}`；`SubsonicResponse<T>` 错误/成功信封。全部 `serde::{Serialize,Deserialize}`，字段对齐 OpenSubsonic + spec §6。

**范围**：定义**跨服务端/客户端共用的纯数据类型**，无逻辑。字段命名与 OpenSubsonic 一致（便于 API 层直接序列化）。为后续 OpenAPI 生成打基础。

**DoD**：类型编译通过；serde round-trip 测试（含 JSON 与 OpenSubsonic 字段名）通过。

<details><summary>交接提示词</summary>

```
[通用前缀]
任务：T1 contract crate 共享 DTO 类型。
在 contract/ 建 crate，定义跨端复用的纯数据类型（无业务逻辑），全部 serde Serialize/Deserialize：
- 媒体：Artist, Album, Track, Genre（字段对齐 OpenSubsonic getSong/getAlbum 等 + spec §6 tracks/albums/artists 列）。
- 歌单：Playlist(含 owner_id, folder_id), PlaylistFolder(含 owner_id, parent_id)。
- 用户：User, Role。
- 访问控制：AccessRule{scope_type: track|album|artist|genre, scope_id}, Principal{type: user|role, id}。
- 流：StreamRequest{id, format, max_bitrate}。
- 信封：SubsonicResponse<T> 成功/错误统一结构。
先写 serde round-trip 失败测试（JSON + OpenSubsonic 字段名），再定义类型使其通过。
DoD：编译通过，round-trip 测试绿。参考 spec §6/§9 的字段。
```
</details>

---

## Task 2：`index` 模块 — SQLite 模式、迁移与数据访问

**依赖**：T0, T1
**Files**：`server/src/index/mod.rs`、`server/src/index/schema.rs`、`server/migrations/*.sql`、`server/src/index/{repo_media,repo_playlist,repo_user,repo_access,repo_annotation}.rs`、`server/tests/index_test.rs`
**Interfaces · Consumes**：T1 的 DTO。**Produces**：仓储 API，如 `MediaRepo::upsert_track/get_album/list_albums/search(fts)`；`PlaylistRepo`（含 folder 树）；`UserRepo`/`RoleRepo`；`AccessRepo::rules_for(track)`；`AnnotationRepo`。连接池 `SqlitePool`（WAL）。

**范围**：建全部表（spec §6：users, roles, user_roles, artists, albums, tracks, annotations, tag_overrides, playlist_folders, playlists, playlist_tracks, access_rules, access_rule_grants, transcode_cache, scan_state），sqlx 迁移，WAL，FTS5 搜索表 + 触发器同步。类型化仓储层。**不含** HTTP、不含扫描/转码逻辑。

**DoD**：迁移可应用；各仓储 CRUD + FTS5 搜索 + 歌单文件夹树查询测试通过（用临时 SQLite 文件）。

<details><summary>交接提示词</summary>

```
[通用前缀]
任务：T2 index 模块（SQLite 模式 + 迁移 + 数据访问层）。
依赖 contract crate 的 DTO。用 sqlx + SQLite（开 WAL）实现：
- 迁移建 spec §6 全部表：users, roles, user_roles, artists, albums, tracks, annotations, tag_overrides, playlist_folders(自引用 parent_id, owner_id), playlists(owner_id, folder_id), playlist_tracks, access_rules, access_rule_grants, transcode_cache, scan_state。
- FTS5 虚拟表覆盖曲目/专辑/艺人名 + 同步触发器，支撑 search3。
- 仓储层：MediaRepo(upsert/get/list/search)、PlaylistRepo(含文件夹树 CRUD 与移动)、UserRepo/RoleRepo、AnnotationRepo、AccessRepo(按曲目查适用规则)。
先为每个仓储行为写失败测试（临时 DB 文件），再实现。仅数据层，无 HTTP。
DoD：迁移应用成功；CRUD + FTS + 文件夹树测试全绿。SQLite 放本地磁盘（红线）。
```
</details>

---

## Task 3：`storage` 模块 — Garage/S3 客户端

**依赖**：T1, T2 · **可并行**：T6
**Files**：`server/src/storage/mod.rs`、`server/src/storage/garage.rs`、`server/tests/storage_test.rs`
**Interfaces · Produces**：`trait ObjectStore { list(prefix), get(key), get_range(key, range), put(key, bytes), delete(key), head(key)->{etag,size} }` + Garage 实现。trait 便于用假实现测试。

**范围**：选定 `object_store` 或 `aws-sdk-s3`（**选定后写进 ADR，勿再换**）；实现列举（分页）、GET（全量 + Range）、PUT、DELETE、HEAD（取 etag/size）。trait 抽象。**不含**扫描/转码/HTTP。

**DoD**：针对本地 MinIO/Garage（或容器）的集成测试通过：put→head(etag)→get_range→list→delete。

<details><summary>交接提示词</summary>

```
[通用前缀]
任务：T3 storage 模块（Garage/S3 客户端）。
在 object_store 与 aws-sdk-s3 中二选一（在 docs/adr 追加一条记录你的选择与理由，之后不再更换）。
定义 trait ObjectStore：list(prefix 分页)、get(key)、get_range(key, byte range)、put(key, bytes)、delete(key)、head(key)->{etag,size}；提供 Garage 实现。trait 要能用内存假实现替换以便单测。
先写失败测试（用假实现做单测 + 一个针对本地 MinIO/Garage 的集成测试），再实现。
DoD：集成测试 put→head→get_range→list→delete 全绿。仅存储层，不碰扫描/转码。
```
</details>

---

## Task 6：认证与用户/角色管理

**依赖**：T1, T2 · **可并行**：T3
**Files**：`server/src/auth/mod.rs`、`server/src/auth/{password,subsonic,bearer,middleware}.rs`、`server/src/auth/user_admin.rs`、`server/tests/auth_test.rs`
**Interfaces · Produces**：`CurrentUser` 提取器（axum extractor）；`verify_subsonic(u,t,s|p)`；`issue_bearer/verify_bearer`；`UserAdmin::{create_user,update,delete,change_password,create_role,assign_role,...}`；`is_admin(user)`。

**范围**：密码可逆加密存储；OpenSubsonic 认证（`u/t/s` token 或明文 `p`，走 HTTP）；自研 Bearer 令牌签发/校验；axum 中间件把请求解析成 `CurrentUser`（含角色）；用户/角色管理逻辑。**不含** HTTP 路由注册（暴露 handler fn 供 T7 挂载）。

**DoD**：Subsonic token 与明文认证、Bearer 签发/校验、用户/角色 CRUD、`is_admin` 判定测试通过；错误凭证被拒。

<details><summary>交接提示词</summary>

```
[通用前缀]
任务：T6 认证与用户/角色管理。
依赖 index 的 UserRepo/RoleRepo 与 contract 类型。实现：
- 密码可逆加密存储（用于支持 Subsonic token 校验，同 Navidrome 思路）。
- OpenSubsonic 认证：校验 u + t(=md5(密码+盐)) + s，或明文 p；支持纯 HTTP。
- 自研 Bearer 令牌：签发与校验（会话令牌）。
- axum extractor CurrentUser：从请求解析用户身份与角色。
- 用户/角色管理逻辑：create/update/delete user、change_password、create/delete role、assign/unassign role、is_admin。
只暴露 handler 函数供 T7 注册路由，自己不改路由树（并行协调规则）。
先写失败测试（含错误凭证被拒、token 与明文两条路径、角色分配），再实现。
DoD：认证/用户/角色测试全绿。授权判定要可靠，服务端强制。
```
</details>

---

## Task 4：`scanner` 模块 — 入库 / 扫描

**依赖**：T2, T3 · **可并行**：T5
**Files**：`server/src/scanner/mod.rs`、`server/src/scanner/{tags,cover,incremental}.rs`、`server/tests/scanner_test.rs`
**Interfaces · Consumes**：`ObjectStore`(T3)、`MediaRepo`(T2)。**Produces**：`Scanner::scan(prefix?) -> ScanReport`；`ScanStatus`（供 getScanStatus）；供 startScan 调用的入口。

**范围**：用 `symphonia`/`lofty` **Range 读文件头**解析标签；抽取内嵌封面并 `put` 到 Garage；增量扫描（列举 bucket，比对 `(key,etag,size)` 判新增/改/删）；写 index；更新 `scan_state`（断点）；tokio 信号量限流。**不含** HTTP。

**DoD**：对含若干 FLAC 的 fixture bucket 扫描 → index 正确填充（含封面 key）；二次扫描无变更时 no-op；删除文件后标记删除。

<details><summary>交接提示词</summary>

```
[通用前缀]
任务：T4 scanner 模块（入库/扫描）。
依赖 storage 的 ObjectStore 与 index 的 MediaRepo。实现：
- 用 symphonia/lofty 通过 Range 读音频文件头解析标签（不下载整文件）。
- 抽取内嵌封面，单独 put 到 Garage，记录 cover_key。
- 增量扫描：list bucket，用 (key,etag,size) 比对 tracks 表 → 新增/etag 变化/已删除 三类处理。
- 更新 scan_state 支持断点；tokio 信号量限流并发。
- 暴露 scan(prefix?) 与 scan_status 供后续 startScan/getScanStatus 使用（不自建 HTTP 路由）。
先写失败测试：用假 ObjectStore 提供若干 FLAC fixture，断言 index 填充、二次扫描 no-op、删除被标记。再实现。
DoD：扫描/增量/删除测试全绿。绝不把整文件读进内存（红线）。
```
</details>

---

## Task 5：`transcode` 模块 — FFmpeg 管线

**依赖**：T2, T3 · **可并行**：T4
**Files**：`server/src/transcode/mod.rs`、`server/src/transcode/{ffmpeg,cache,decision}.rs`、`server/tests/transcode_test.rs`
**Interfaces · Consumes**：`ObjectStore`(T3)、`transcode_cache` 仓储(T2)。**Produces**：`Transcoder::stream(track, target) -> impl Stream<Bytes>`；`should_transcode(track, target) -> bool`（透传判定）；缓存键 `transcode/{id}/{fmt}_{br}.{ext}`。

**范围**：透传判定（原格式兼容则不转码）；查缓存命中则从 Garage 流式返回；未命中则 FFmpeg 子进程实时转码，**边发边落临时文件**，成功后 put 到 Garage + 写缓存登记，**中途断开丢弃**；信号量限流；有界缓冲。可选 LRU 淘汰。**不含** HTTP 路由。

**DoD**：转码 fixture → 输出可播；二次请求命中缓存；模拟中断 → 缓存中无不完整对象；透传判定正确。

<details><summary>交接提示词</summary>

```
[通用前缀]
任务：T5 transcode 模块（FFmpeg 管线，按需+缓存）。
依赖 storage 的 ObjectStore 与 index 的 transcode_cache 仓储。实现：
- should_transcode 透传判定：客户端要原格式或原文件已兼容目标 → 不转码直接透传。
- 缓存查找：命中则从 Garage 流式返回缓存对象（键 transcode/{track_id}/{format}_{bitrate}.{ext}）。
- 未命中：FFmpeg 子进程把 Garage 音频流转成目标（FLAC/ALAC→AAC/Opus），一边发给调用方一边落临时文件；成功后 put Garage + 写缓存登记。
- 中途失败/断开：丢弃临时文件，绝不缓存不完整产物（红线）。
- tokio 信号量限制并发 FFmpeg 数；流式有界缓冲，不把整文件读进内存。
暴露 stream/should_transcode 供 T7 的 /rest/stream 调用，不自建路由。
先写失败测试（转码成功、二次命中缓存、模拟中断后缓存无残留、透传判定），再实现。
DoD：上述测试全绿。
```
</details>

---

## Task 7：API 层 — OpenSubsonic 兼容子集

**依赖**：T2, T3, T4, T5, T6
**Files**：`server/src/api/mod.rs`（路由注册中心）、`server/src/api/{system,browsing,search,playlist,media,annotation,scan,user}.rs`、`server/src/api/response.rs`（XML+JSON 信封）、`server/tests/opensubsonic_test.rs`
**Interfaces · Consumes**：全部下游模块。**Produces**：完整 OpenSubsonic 兼容路由树。

**范围**：实现 spec §9 兼容子集全部接口（system/browsing/search3/playlist/media stream+download+coverart/annotation/scan/user 管理）；XML 与 JSON（`f=json`）双响应；挂载 T6 认证中间件与各模块 handler。此任务是集成点，负责路由注册。**先不做**访问控制过滤（T9）与扩展接口（T8）。

**DoD**：OpenSubsonic 一致性测试通过；真实客户端（Amperfy/play:Sub）能登录、浏览、播放（含转码流）。

<details><summary>交接提示词</summary>

```
[通用前缀]
任务：T7 API 层 OpenSubsonic 兼容子集（集成任务）。
挂载 auth 中间件与 index/storage/scanner/transcode 各模块，实现 spec §9 兼容子集全部接口：
- system: ping, getLicense, getOpenSubsonicExtensions
- browsing: getArtists, getArtist, getAlbum, getSong, getAlbumList2, getGenres, getIndexes
- search: search3（走 FTS5）
- playlist: getPlaylists, getPlaylist, createPlaylist, updatePlaylist, deletePlaylist
- media: stream（调 transcode）, download, getCoverArt
- annotation: star, unstar, setRating, scrobble
- scan: getScanStatus, startScan（调 scanner）
- user: getUser, getUsers, createUser, updateUser, deleteUser, changePassword（调 T6）
统一 subsonic-response 信封，支持 XML 与 f=json。本任务负责 api/mod.rs 路由注册中心。
暂不做曲库访问控制过滤（T9）与自研扩展（T8）。
先写 OpenSubsonic 一致性失败测试（每个端点的响应结构），再实现。
DoD：一致性测试全绿；真实 OpenSubsonic 客户端能登录/浏览/播放。绝不破坏兼容（红线）。
```
</details>

---

## Task 8：自研扩展接口

**依赖**：T7, T6, T2 · **可并行**：T9
**Files**：`server/src/api/ext/{mod,playlist_tree,library,access,role,scan}.rs`、`server/tests/ext_test.rs`
**Interfaces · Consumes**：T2 仓储、T6 授权、T3/T4 入库。**Produces**：`/rest/ext/*` 路由；`getOpenSubsonicExtensions` 声明这些扩展。

**范围**：多级歌单（getPlaylistTree、folder CRUD、movePlaylist、moveFolder）；库管理写（uploadTrack multipart→Garage+入库、updateTags 覆盖层、deleteTrack、moveTrack）；访问控制管理（setAccessRule、getAccessRules、deleteAccessRule）；角色管理（getRoles、createRole、deleteRole、assignRole、unassignRole）；扫描范围触发。全部命名空间隔离于 `/rest/ext/*`，管理类接口仅管理员。在 `getOpenSubsonicExtensions` 中声明。

**DoD**：扩展接口测试通过；`updateTags` 走覆盖层不动原文件；非管理员调管理接口被拒；`getOpenSubsonicExtensions` 列出扩展。

<details><summary>交接提示词</summary>

```
[通用前缀]
任务：T8 自研扩展接口（/rest/ext/*）。
依赖 T7 已建的 api 路由中心与 T2/T6/T3/T4。实现（全部挂在 /rest/ext/* 命名空间）：
- 多级歌单：getPlaylistTree, createPlaylistFolder, updatePlaylistFolder, deletePlaylistFolder, movePlaylist, moveFolder（按 owner_id 隔离用户各自的树）。
- 库管理写：uploadTrack(multipart→put Garage→调 scanner 入库), updateTags(写 tag_overrides 覆盖层，不动原文件；另提供显式"写回文件"操作), deleteTrack, moveTrack。
- 访问控制管理：setAccessRule(scope+允许名单), getAccessRules, deleteAccessRule。
- 角色管理：getRoles, createRole, deleteRole, assignRole, unassignRole。
- 扫描范围触发。
管理类接口仅管理员可调用；在 getOpenSubsonicExtensions 声明这些扩展。
先写失败测试（含非管理员被拒、updateTags 不改原文件、树操作），再实现。
DoD：扩展测试全绿；不破坏已有兼容接口。
```
</details>

---

## Task 9：曲库访问控制强制

**依赖**：T7, T6, T2 · **可并行**：T8
**Files**：`server/src/index/access.rs`（可见性查询）、修改 `server/src/api/{browsing,search,media,playlist}.rs` 注入过滤、`server/tests/access_control_test.rs`
**Interfaces · Consumes**：`CurrentUser`(T6)、`AccessRepo`(T2)。**Produces**：`visible_filter(user) -> SQL 条件` / `can_access(user, track) -> bool`，注入所有曲库读路径。

**范围**：实现**多级作用域、最具体优先、查询时评估、管理员绕过**的可见性判定；把过滤注入 browsing/search3/stream/download/getCoverArt/playlist 展开等所有曲库读路径；新入库曲目自动继承专辑/艺人规则（因查询时评估）。

**DoD**：无授权用户看不到/放不了受限曲目；管理员全见；给专辑设规则后，新扫入该专辑的曲目自动受限；最具体规则优先（曲目 > 专辑 > 艺人 > 流派）。

<details><summary>交接提示词</summary>

```
[通用前缀]
任务：T9 曲库访问控制强制（跨切面）。
依赖 T6 的 CurrentUser 与 T2 的 access_rules/access_rule_grants。实现 spec §6「曲库访问控制模型」：
- 可见性判定：默认开放；某曲目在其 曲目/专辑/艺人/流派 层级存在限制规则时，仅允许名单内用户/角色可见；最具体作用域优先；管理员绕过。
- 查询时评估（不逐曲固化），使新入库曲目自动继承其专辑/艺人规则。
- 把过滤注入所有曲库读路径：getArtists/getArtist/getAlbum/getSong/getAlbumList2/getIndexes、search3、stream/download/getCoverArt、歌单展开。
先写失败测试：受限曲目对无授权用户不可见/不可播、对管理员可见；给专辑设规则后新扫入曲目自动受限；曲目级规则覆盖专辑级。再实现。
注意与 T8 并行：你改查询/过滤层，T8 加新端点，尽量不冲突；共享文件改动最小化。
DoD：访问控制测试全绿；授权服务端强制，客户端无法绕过（红线）。
```
</details>

---

## Task 10：OpenAPI 生成 + 部署收尾

**依赖**：T7, T1
**Files**：`contract/build.rs` 或 `xtask/`（OpenAPI 生成）、`openapi.yaml`（产物）、`docker-compose.yml`（补 FFmpeg + 完善）、`server/src/setup.rs`（首启建管理员）、`Dockerfile`（musl 静态构建）、`README.md`
**Interfaces · Consumes**：T1 契约类型、T7 API。**Produces**：`openapi.yaml`；一键部署；首启引导。

**范围**：从 `contract` + API 生成 OpenAPI（供 web 端生成 TS 类型）；完善 docker-compose（server+garage+ffmpeg 三件）；首启无用户时通过配置或简单 Web 设置页建管理员；musl 静态构建 Dockerfile；README 快速上手。

**DoD**：`docker compose up` 起可用服务；`openapi.yaml` 生成且与实现一致；首启创建管理员流程可用；README 能让小白照做起服务。

<details><summary>交接提示词</summary>

```
[通用前缀]
任务：T10 OpenAPI 生成 + 部署收尾。
依赖 T1 契约与 T7 API。实现：
- 从 contract + API 定义生成 openapi.yaml（供 web 端生成 TS 类型，保证类型全平台同源）。
- 完善 docker-compose.yml：server + garage + ffmpeg 三服务，一条 docker compose up 全起。
- 首启引导：无用户时通过配置或简单 Web 设置页创建管理员。
- Dockerfile：musl 静态构建单二进制。
- README：小白快速上手（含默认明文 HTTP，TLS 为进阶可选）。
先写测试/校验（openapi 生成非空且覆盖主要端点、首启建管理员逻辑、compose 配置校验），再实现。
DoD：docker compose up 起可用服务；openapi.yaml 与实现一致；首启建管理员可用；README 可照做。
```
</details>

---

## Self-Review（对照 spec §1–§13）

- §1 多用户/家庭/访问控制 → T2(表)/T6(用户角色)/T9(访问控制) ✓
- §3 跨平台 contract/core → T1（core 与各客户端为后续子项目，非本计划）✓
- §5 架构/技术栈/存储位置 → T0/T2/T3 ✓；无 Redis/Postgres 红线写入 AGENTS.md+ADR ✓
- §6 数据模型（含多级歌单、访问控制表）→ T2 ✓
- §7 双路径入库 → T4(扫描) + T8(uploadTrack) ✓
- §8 按需转码+缓存+不缓存残缺 → T5 ✓
- §9 OpenSubsonic 子集 + 扩展 → T7 + T8 ✓
- §10 认证（Subsonic+Bearer+HTTP）→ T6 ✓
- §11 配置/部署/可观测 → T0(config/日志/healthz) + T10(compose/首启/musl) ✓
- §12 排除项（HLS/metrics/预转码/WASM）未进任务 ✓
- 无占位符；接口 Consumes/Produces 命名跨任务一致 ✓

## 执行入口

- **Claude Code 单会话执行**：用 `superpowers:subagent-driven-development`（每任务派新 subagent + 评审）或 `superpowers:executing-plans`。
- **多工具并行执行**：按波次把各任务的「交接提示词」分发给不同 AI 工具；同波次并行，跨波次遵守依赖顺序与"共享文件由集成任务改"的协调规则。
