# 服务端任务 · 可直接复制的交接提示词

按波次执行。**同一波次内的任务可并行**（丢给不同 AI 同时做），**跨波次必须按顺序**（上一波全绿再开下一波）。每条提示词自包含，复制整块即可。

- 波次 A（串行）：T0 → T1 → T2
- 波次 B（并行）：T3 ‖ T6
- 波次 C（并行）：T4 ‖ T5
- 波次 D：T7
- 波次 E（并行）：T8 ‖ T9
- 波次 F：T10

并行协调铁律：并行任务只改各自模块目录；共享文件（如 `server/src/api/mod.rs` 路由树）由后面的集成任务（T7）负责，前置任务只暴露 `pub fn`。

---

## 波次 A · T0（先做，独占）

```
你在为一个自托管家庭音乐服务的 Rust 服务端工作。仓库根有 AGENTS.md（项目宪法，含架构红线与强制工作流），docs/superpowers/specs/2026-07-10-music-server-design.md（设计），docs/superpowers/plans/2026-07-10-music-server.md（本任务详情 T0）。开始前必须读完这三份。

任务 T0：仓库脚手架与治理骨架。建立 server/ 的 Cargo workspace 与 crate 骨架：
- Config：从 TOML 文件 + 环境变量覆盖加载，字段含监听地址/端口、Garage 端点/bucket/凭证、SQLite 路径、转码缓存上限、扫描间隔、默认转码格式/码率、FFmpeg 路径、可选 TLS 证书，全部带合理默认。
- axum 应用骨架，暴露 GET /healthz(200) 与 OpenSubsonic GET /rest/ping（返回标准 subsonic-response ok，支持 f=json）。
- tracing 结构化日志，级别可配。
- .github/workflows/ci.yml：cargo fmt --check、clippy -D warnings、test。
- docker-compose.yml：server + garage 两服务骨架（FFmpeg 后续任务加）。
不引入任何数据库/业务逻辑。

规则：强制 TDD——先为 /healthz 和 /rest/ping 写失败集成测试→跑红→最小实现→跑绿→提交（Conventional Commits，中文信息可）。只做本任务范围内的事。完成前 cargo test / cargo clippy -- -D warnings / cargo fmt --check 必须全绿。不确定就停下来问，不要臆测或偏离 spec。
DoD：cargo test 绿，服务可启动并响应 /healthz 与 /rest/ping，CI 就绪。
```

---

## 波次 A · T1（T0 完成后）

```
你在为一个自托管家庭音乐服务的 Rust 服务端工作。仓库根有 AGENTS.md（项目宪法），docs/superpowers/specs/2026-07-10-music-server-design.md（设计，重点看 §6/§9），docs/superpowers/plans/2026-07-10-music-server.md（本任务 T1）。开始前必须读完。

任务 T1：contract crate 共享 DTO 类型。在 contract/ 建 crate，定义跨服务端/客户端复用的纯数据类型（无业务逻辑），全部 serde Serialize/Deserialize，字段命名对齐 OpenSubsonic：
- 媒体：Artist, Album, Track, Genre（字段对齐 OpenSubsonic getSong/getAlbum + spec §6 tracks/albums/artists 列）。
- 歌单：Playlist(含 owner_id, folder_id), PlaylistFolder(含 owner_id, parent_id)。
- 用户：User, Role。
- 访问控制：AccessRule{scope_type: track|album|artist|genre, scope_id}, Principal{type: user|role, id}。
- 流：StreamRequest{id, format, max_bitrate}。
- 信封：SubsonicResponse<T> 成功/错误统一结构。

规则：强制 TDD——先写 serde round-trip 失败测试（JSON + OpenSubsonic 字段名）→跑红→定义类型→跑绿→提交。只做本任务范围。cargo test/clippy -D warnings/fmt --check 必须全绿。不确定就问。
DoD：编译通过，round-trip 测试全绿。
```

---

## 波次 A · T2（T1 完成后）

```
你在为一个自托管家庭音乐服务的 Rust 服务端工作。仓库根有 AGENTS.md（宪法，注意 SQLite 放本地磁盘、禁止 Postgres/Redis 的红线），docs/superpowers/specs/2026-07-10-music-server-design.md（设计 §6），docs/superpowers/plans/2026-07-10-music-server.md（本任务 T2）。开始前必须读完。

任务 T2：index 模块（SQLite 模式 + 迁移 + 数据访问层）。依赖 contract crate 的 DTO。用 sqlx + SQLite（开 WAL）实现：
- 迁移建 spec §6 全部表：users, roles, user_roles, artists, albums, tracks, annotations, tag_overrides, playlist_folders(自引用 parent_id, owner_id), playlists(owner_id, folder_id), playlist_tracks, access_rules, access_rule_grants, transcode_cache, scan_state。
- FTS5 虚拟表覆盖曲目/专辑/艺人名 + 同步触发器，支撑 search3。
- 仓储层：MediaRepo(upsert/get/list/search)、PlaylistRepo(含文件夹树 CRUD 与移动)、UserRepo/RoleRepo、AnnotationRepo、AccessRepo(按曲目查适用规则)。
仅数据层，无 HTTP。

规则：强制 TDD——先为每个仓储行为写失败测试（临时 DB 文件）→跑红→实现→跑绿→提交。只做本任务范围。cargo test/clippy -D warnings/fmt --check 必须全绿。SQLite 必须放本地磁盘（红线）。不确定就问。
DoD：迁移应用成功；CRUD + FTS + 文件夹树测试全绿。
```

---

## 波次 B · T3（与 T6 并行）

```
你在为一个自托管家庭音乐服务的 Rust 服务端工作。仓库根有 AGENTS.md（宪法），docs/superpowers/specs/2026-07-10-music-server-design.md（设计），docs/superpowers/plans/2026-07-10-music-server.md（本任务 T3）。开始前必须读完。

任务 T3：storage 模块（Garage/S3 客户端）。在 object_store 与 aws-sdk-s3 中二选一（在 docs/adr 追加一条记录你的选择与理由，之后不再更换）。
- 定义 trait ObjectStore：list(prefix 分页)、get(key)、get_range(key, byte range)、put(key, bytes)、delete(key)、head(key)->{etag,size}；提供 Garage 实现。
- trait 要能用内存假实现替换以便单测。
仅存储层，不碰扫描/转码/HTTP。

并行协调：只改 server/src/storage/ 目录，不动路由树。
规则：强制 TDD——先写失败测试（假实现单测 + 一个针对本地 MinIO/Garage 的集成测试）→跑红→实现→跑绿→提交。cargo test/clippy -D warnings/fmt --check 必须全绿。不确定就问。
DoD：集成测试 put→head→get_range→list→delete 全绿。
```

---

## 波次 B · T6（与 T3 并行）

```
你在为一个自托管家庭音乐服务的 Rust 服务端工作。仓库根有 AGENTS.md（宪法），docs/superpowers/specs/2026-07-10-music-server-design.md（设计 §10），docs/superpowers/plans/2026-07-10-music-server.md（本任务 T6）。开始前必须读完。

任务 T6：认证与用户/角色管理。依赖 index 的 UserRepo/RoleRepo 与 contract 类型。实现：
- 密码可逆加密存储（用于支持 Subsonic token 校验，同 Navidrome 思路）。
- OpenSubsonic 认证：校验 u + t(=md5(密码+盐)) + s，或明文 p；支持纯 HTTP。
- 自研 Bearer 令牌：签发与校验。
- axum extractor CurrentUser：从请求解析用户身份与角色。
- 用户/角色管理逻辑：create/update/delete user、change_password、create/delete role、assign/unassign role、is_admin。
只暴露 handler 函数供后续 T7 注册路由，自己不改路由树。

并行协调：只改 server/src/auth/ 目录，不动路由树。
规则：强制 TDD——先写失败测试（错误凭证被拒、token 与明文两条路径、角色分配）→跑红→实现→跑绿→提交。cargo test/clippy -D warnings/fmt --check 必须全绿。授权必须服务端强制。不确定就问。
DoD：认证/用户/角色测试全绿。
```

---

## 波次 C · T4（与 T5 并行）

```
你在为一个自托管家庭音乐服务的 Rust 服务端工作。仓库根有 AGENTS.md（宪法，注意"绝不把整文件读进内存"红线），docs/superpowers/specs/2026-07-10-music-server-design.md（设计 §7），docs/superpowers/plans/2026-07-10-music-server.md（本任务 T4）。开始前必须读完。

任务 T4：scanner 模块（入库/扫描）。依赖 storage 的 ObjectStore 与 index 的 MediaRepo。实现：
- 用 symphonia/lofty 通过 Range 读音频文件头解析标签（不下载整文件）。
- 抽取内嵌封面，单独 put 到 Garage，记录 cover_key。
- 增量扫描：list bucket，用 (key,etag,size) 比对 tracks 表 → 新增/etag 变化/已删除 三类处理。
- 更新 scan_state 支持断点；tokio 信号量限流并发。
- 暴露 scan(prefix?) 与 scan_status 供后续 startScan/getScanStatus 使用（不自建 HTTP 路由）。

并行协调：只改 server/src/scanner/ 目录。
规则：强制 TDD——先写失败测试（用假 ObjectStore 提供若干 FLAC fixture，断言 index 填充、二次扫描 no-op、删除被标记）→跑红→实现→跑绿→提交。cargo test/clippy -D warnings/fmt --check 必须全绿。绝不把整文件读进内存。不确定就问。
DoD：扫描/增量/删除测试全绿。
```

---

## 波次 C · T5（与 T4 并行）

```
你在为一个自托管家庭音乐服务的 Rust 服务端工作。仓库根有 AGENTS.md（宪法，注意"绝不缓存不完整转码产物""流式有界缓冲"红线），docs/superpowers/specs/2026-07-10-music-server-design.md（设计 §8），docs/superpowers/plans/2026-07-10-music-server.md（本任务 T5）。开始前必须读完。

任务 T5：transcode 模块（FFmpeg 管线，按需+缓存）。依赖 storage 的 ObjectStore 与 index 的 transcode_cache 仓储。实现：
- should_transcode 透传判定：客户端要原格式或原文件已兼容目标 → 不转码直接透传。
- 缓存查找：命中则从 Garage 流式返回缓存对象（键 transcode/{track_id}/{format}_{bitrate}.{ext}）。
- 未命中：FFmpeg 子进程把 Garage 音频流转成目标（FLAC/ALAC→AAC/Opus），一边发给调用方一边落临时文件；成功后 put Garage + 写缓存登记。
- 中途失败/断开：丢弃临时文件，绝不缓存不完整产物。
- tokio 信号量限制并发 FFmpeg 数；流式有界缓冲，不把整文件读进内存。
暴露 stream/should_transcode 供后续 T7 的 /rest/stream 调用，不自建路由。

并行协调：只改 server/src/transcode/ 目录。
规则：强制 TDD——先写失败测试（转码成功、二次命中缓存、模拟中断后缓存无残留、透传判定）→跑红→实现→跑绿→提交。cargo test/clippy -D warnings/fmt --check 必须全绿。不确定就问。
DoD：上述测试全绿。
```

---

## 波次 D · T7（B、C 全部完成后，集成，独占）

```
你在为一个自托管家庭音乐服务的 Rust 服务端工作。仓库根有 AGENTS.md（宪法，注意"禁止破坏 OpenSubsonic 兼容"红线），docs/superpowers/specs/2026-07-10-music-server-design.md（设计 §9），docs/superpowers/plans/2026-07-10-music-server.md（本任务 T7）。开始前必须读完。

任务 T7：API 层 OpenSubsonic 兼容子集（集成任务，负责 api/mod.rs 路由注册中心）。挂载 auth 中间件与 index/storage/scanner/transcode 各模块，实现 spec §9 兼容子集全部接口：
- system: ping, getLicense, getOpenSubsonicExtensions
- browsing: getArtists, getArtist, getAlbum, getSong, getAlbumList2, getGenres, getIndexes
- search: search3（走 FTS5）
- playlist: getPlaylists, getPlaylist, createPlaylist, updatePlaylist, deletePlaylist
- media: stream（调 transcode）, download, getCoverArt
- annotation: star, unstar, setRating, scrobble
- scan: getScanStatus, startScan（调 scanner）
- user: getUser, getUsers, createUser, updateUser, deleteUser, changePassword（调 T6）
统一 subsonic-response 信封，支持 XML 与 f=json。
暂不做曲库访问控制过滤（T9）与自研扩展（T8）。

规则：强制 TDD——先写 OpenSubsonic 一致性失败测试（每个端点的响应结构）→跑红→实现→跑绿→提交。cargo test/clippy -D warnings/fmt --check 必须全绿。绝不破坏 OpenSubsonic 兼容。不确定就问。
DoD：一致性测试全绿；真实 OpenSubsonic 客户端（Amperfy/play:Sub）能登录/浏览/播放。
```

---

## 波次 E · T8（与 T9 并行）

```
你在为一个自托管家庭音乐服务的 Rust 服务端工作。仓库根有 AGENTS.md（宪法，扩展走 /rest/ext/* 并在 getOpenSubsonicExtensions 声明），docs/superpowers/specs/2026-07-10-music-server-design.md（设计 §9），docs/superpowers/plans/2026-07-10-music-server.md（本任务 T8）。开始前必须读完。

任务 T8：自研扩展接口（/rest/ext/*）。依赖 T7 已建的 api 路由中心与 T2/T6/T3/T4。实现（全部挂在 /rest/ext/* 命名空间）：
- 多级歌单：getPlaylistTree, createPlaylistFolder, updatePlaylistFolder, deletePlaylistFolder, movePlaylist, moveFolder（按 owner_id 隔离用户各自的树）。
- 库管理写：uploadTrack(multipart→put Garage→调 scanner 入库), updateTags(写 tag_overrides 覆盖层，不动原文件；另提供显式"写回文件"操作), deleteTrack, moveTrack。
- 访问控制管理：setAccessRule(scope+允许名单), getAccessRules, deleteAccessRule。
- 角色管理：getRoles, createRole, deleteRole, assignRole, unassignRole。
- 扫描范围触发。
管理类接口仅管理员可调用；在 getOpenSubsonicExtensions 声明这些扩展。

并行协调：你加新端点文件（server/src/api/ext/），尽量不动 T9 要改的查询/过滤层；共享文件改动最小化。
规则：强制 TDD——先写失败测试（非管理员被拒、updateTags 不改原文件、树操作）→跑红→实现→跑绿→提交。cargo test/clippy -D warnings/fmt --check 必须全绿。不确定就问。
DoD：扩展测试全绿；不破坏已有兼容接口。
```

---

## 波次 E · T9（与 T8 并行）

```
你在为一个自托管家庭音乐服务的 Rust 服务端工作。仓库根有 AGENTS.md（宪法，"授权服务端强制、客户端不可绕过"红线），docs/superpowers/specs/2026-07-10-music-server-design.md（设计 §6 曲库访问控制模型），docs/superpowers/plans/2026-07-10-music-server.md（本任务 T9）。开始前必须读完。

任务 T9：曲库访问控制强制（跨切面）。依赖 T6 的 CurrentUser 与 T2 的 access_rules/access_rule_grants。实现 spec §6「曲库访问控制模型」：
- 可见性判定：默认开放；某曲目在其 曲目/专辑/艺人/流派 层级存在限制规则时，仅允许名单内用户/角色可见；最具体作用域优先；管理员绕过。
- 查询时评估（不逐曲固化），使新入库曲目自动继承其专辑/艺人规则。
- 把过滤注入所有曲库读路径：getArtists/getArtist/getAlbum/getSong/getAlbumList2/getIndexes、search3、stream/download/getCoverArt、歌单展开。

并行协调：你改查询/过滤层（server/src/index/access.rs 及各 browsing/search/media handler 内的过滤），T8 加新端点，尽量不冲突。
规则：强制 TDD——先写失败测试（受限曲目对无授权用户不可见/不可播、对管理员可见；给专辑设规则后新扫入曲目自动受限；曲目级规则覆盖专辑级）→跑红→实现→跑绿→提交。cargo test/clippy -D warnings/fmt --check 必须全绿。授权服务端强制。不确定就问。
DoD：访问控制测试全绿。
```

---

## 波次 F · T10（E 完成后，收尾）

```
你在为一个自托管家庭音乐服务的 Rust 服务端工作。仓库根有 AGENTS.md（宪法），docs/superpowers/specs/2026-07-10-music-server-design.md（设计 §11），docs/superpowers/plans/2026-07-10-music-server.md（本任务 T10）。开始前必须读完。

任务 T10：OpenAPI 生成 + 部署收尾。依赖 T1 契约与 T7 API。实现：
- 从 contract + API 定义生成 openapi.yaml（供 web 端生成 TS 类型，保证类型全平台同源）。
- 完善 docker-compose.yml：server + garage + ffmpeg 三服务，一条 docker compose up 全起。
- 首启引导：无用户时通过配置或简单 Web 设置页创建管理员。
- Dockerfile：musl 静态构建单二进制。
- README：小白快速上手（含默认明文 HTTP，TLS 为进阶可选）。

规则：强制 TDD——先写测试/校验（openapi 生成非空且覆盖主要端点、首启建管理员逻辑、compose 配置校验）→跑红→实现→跑绿→提交。cargo test/clippy -D warnings/fmt --check 必须全绿。不确定就问。
DoD：docker compose up 起可用服务；openapi.yaml 与实现一致；首启建管理员可用；README 可照做。
```
