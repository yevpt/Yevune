# M4 曲库发现工作台真实验收

日期：2026-07-17（Asia/Shanghai）
基线：`fb1dc36`
冒烟起始 HEAD：`daa3e7523b133396934beec146d165d9ae746a1e`
缺陷修复 HEAD：`225ede3`（另含 `d5afdc5`）

## 真实服务与数据

- Docker 29.4.0 / OrbStack；Garage `dxflrs/garage:v2.3.0`，端口 `3900`（S3）与 `3903`（admin）。
- 按仓库流程运行 `docker compose up -d garage`，复用本 worktree 隔离的 Garage volume；用当前 HEAD 的 `cargo run --manifest-path server/Cargo.toml --bin yevune-server` 在 `127.0.0.1:4533` 启动 Rust server。
- `GET /healthz` 返回成功；标准 `/rest/ping.view` 返回 HTTP 200，脱敏摘要：`status=ok, openSubsonic=true, type=yevune-server, version=1.16.1`。
- FFmpeg 8.1.1 生成 130 个 1 秒 FLAC。每个文件具有唯一 `M4 Album NNN` / `M4 Track NNN` / `M4 Artist NNN`，并轮换 Rock/Jazz/Classical 与 2000–2024 年份；经 `/rest/ext/uploadTrack` 逐个上传到 `library/m4/`，130/130 响应均为 `ok`。
- SQLite 实测为 130 tracks / 130 albums。标准 `getAlbumList2.view?type=alphabeticalByName&offset=120&size=60` 返回 10 张（要求至少 5 张）。
- 所有 admin/member 密码均运行时随机生成，只存在临时 shell/进程环境；本报告、Git 与 shell 命令历史均不包含凭证值。

## 真实角色与服务端授权

- 创建临时 member `m4-member-1784217249-10194`；member 的 ping 与 offset 120 曲库浏览均成功（返回 10 张）。
- member 请求 `/rest/startScan.view` 与 `/rest/ext/uploadTrack` 均得到 OpenSubsonic `status=failed, error.code=50`，证明服务端而非客户端隐藏强制授权。
- UI 真实登录 member 后，AX 树只有“曲库/歌单”，工具栏只有边栏和 member 账户；不存在管理、用户、角色、访问控制、导入、扫描或任务入口。admin 对照会话全部入口可见。
- 审查修复复验在真实库把 alphabetical 首条 `M4 Album 001` 设置为无 grant ACL。member 的 `getAlbumList2(type=alphabeticalByName,size=60)` 在 offset 0/60/120 分别返回 60/60/9 张，首项为 Album 002、末项为 Album 130，受限 Album 001 零命中；证明 offset 按“可见专辑”计数且第 120 条以后仍可达。
- 修复后重新构建临时 app、真实登录复验 member；AX 仍无管理/导入/扫描/任务入口，普通 Sky scroll 后摘要为 `129 张专辑` 并到达末段。证据：`.superpowers/sdd/artifacts/screenshots/m4-member-acl-review.png` 与 `m4-member-acl-129-loaded.png`。
- 验收结束后删除三次流程使用的全部临时 member 与复验 ACL；SQLite 查询 `name LIKE 'm4-%'` 无记录、该 album rule 计数为 0。临时 Rust server 与 Garage 已停止，先前为释放 4533 而暂停的 `music-server-1` 已恢复运行。

## Computer Use UI 冒烟

仓库 `./scripts/tests/run-mac-client-test.sh` 新鲜通过；该脚本验证 launcher，但不生成 `.app`。因此把同一次 `swift run` 构建出的 Mach-O 封装成未提交的临时 `Yevune-M4.app`，并严格使用 Computer Use 技能规定的 `node_repl` + Sky 操作 `dev.yevune.m4-smoke`，没有使用 AppleScript/JXA/System Events。

可靠的 920pt 截图均为 920×672：

- `.superpowers/sdd/artifacts/screenshots/m4-admin-920-library.png`：紧凑命令栏无裁剪，admin 导入/扫描/任务入口可见。
- `.superpowers/sdd/artifacts/screenshots/m4-admin-920-sort-name.png`：切换“专辑名称”后按 001、002、003…稳定排序。
- `.superpowers/sdd/artifacts/screenshots/m4-admin-920-normal-scroll.png`：Sky 普通滚动 8 页，scrollbar=0.968，已越过第 100 张且进程稳定。
- `.superpowers/sdd/artifacts/screenshots/m4-artist-detail.png`：艺人分区按 `M` 分区并能打开详情。
- `.superpowers/sdd/artifacts/screenshots/m4-member-library.png`：member 无任何管理入口。
- `.superpowers/sdd/artifacts/screenshots/m4-member-acl-129-loaded.png`：审查修复后的有效 member 会话，无管理工具栏，受限曲库仍完整加载 129 张可见专辑。

主代理随后接管同一个 `node_repl` 会话补做宽屏：对窗口执行系统 zoom 后，截图 `.superpowers/sdd/artifacts/screenshots/m4-regular-collection-inspector.jpeg` 为 1275×768；AX 同时出现 regular 专属 `60 张专辑`、网格/列表控制、收藏区与 inspector 占位 `选择专辑或艺人`。对 Album 130 执行 AX `打开专辑` 后，截图 `.superpowers/sdd/artifacts/screenshots/m4-regular-album-inspector-after-member-cleanup.jpeg` 的 inspector 显示 `M4 Album 130 / M4 Artist 130` 且收藏区仍保留，证明 regular 主收藏 + inspector 同时构造；其中认证错误发生在 member 已删除之后，只用于结构证据。

交互结果：

- 搜索 `Track` / `Artist` / `Album` 分别出现曲目/艺人/专辑分区和“加载更多”；艺人、专辑续页按钮均真实触发。两字符 `M4` 只匹配曲目是 FTS5 trigram 最短三字符与曲目 `instr` 策略的预期差异，不是缺陷。
- 艺人分区展示 130 个真实艺人，可打开艺人详情；专辑可打开详情，双击曲目后出现唯一播放器条“当前播放：M4 Track 130”。从详情返回浏览页后同一播放器实例与当前曲目仍保留；1 秒曲目在交互期间自然播放结束。
- 筛选浮层真实展示排序、流派、起止年份、应用年份与网格/列表；“最近入库→专辑名称”切换无闪回。
- Sky 对 `LazyVGrid` 暴露的 `AXScrollToBottom` secondary action 在 macOS 27.0 触发 SwiftUICore `ForEachState.item` / `AccessibilityNode.scrollToBeginning` SIGTRAP；同一场景改用正常 `sky.scroll` 连续 8 页不崩。崩溃记录位于 `~/Library/Logs/DiagnosticReports/Yevune-2026-07-16-235739.ips`，作为 macOS 27 AX secondary-action concern 保留，不把它误记为普通滚动通过。

### UI concerns

- 本代理在外接 PG27UQR 上通过“窗口→移动与调整大小→左侧”时 Sky 只返回 97×123 缩略图；主代理接管后通过 zoom 取得约 1275×768 的可靠 regular AX/截图和 inspector 证据。regular 行为通过，但截图并非严格 1280 像素，故保留尺寸精度 concern（自动化 `LibraryPresentation(width: 1280)` 门禁通过）。
- 未完成流派与年份筛选的连续快速切换、compact push/pop 收藏恢复。筛选控件存在、排序切换通过，但这些细项没有足够 AX/截图证据，保持 concern。
- 主代理在 member 被清理后继续操作旧 member UI session 时看到 `CoreError code 40 Wrong username/password`；这是上述 `deleteUser` 后旧 session 再请求的预期结果。清理前的 member 浏览、入口守卫与服务端拒绝证据均已取得。

## 冒烟发现与 RED → GREEN

真实 130 专辑在默认 `newest` 下暴露同秒 `added_at` 无稳定 tie-break：跨页 UI 顺序出现 012…025 后回跳 003…001，并有 offset 重复/遗漏风险。

1. RED：新增 `newest_album_pagination_is_stable_when_timestamps_tie`，固定五张专辑相同时间戳，分三页读取；失败为 actual `[1,2,3,4,5]`、expected `[5,4,3,2,1]`。
2. GREEN：`server/src/index/repo_media.rs` 的 newest 排序改为 `ORDER BY added_at DESC, id DESC`。
3. 验证：目标测试通过，完整 `index_test` 28/28 通过；真实 UI 重启后显示 130、129、128…稳定顺序。
4. 独立提交：`d5afdc5 fix(server): 稳定最近专辑分页顺序`。

最终独立审查随后指出三项 Important，均先验证、再补 RED → GREEN：

1. **ACL 分页语义**：RED `get_album_list2_按可见专辑而非原始曲库分页` 在首条受限、`size=2` 时实际只返回 1 条。GREEN 将统一曲目可见性谓词注入所有 album-list SQL，使“至少一条可见曲目或空专辑”的筛选发生在 `LIMIT/OFFSET` 之前；`frequent/recent` 聚合也只统计可见曲目。目标 HTTP 测试、真实 60/60/9 member 分页均通过；没有全量载入或新增逐条扫描。
2. **全部确定性排序稳定性**：参数化测试 `album_pagination_is_stable_for_every_deterministic_tied_sort` 为 alphabetical name/artist、year、genre、highest、frequent、recent、starred 构造完全相同主排序值并跨三页读取；newest 继续由独立同时间戳测试覆盖。除语义上随机的 `random()` 外，所有列表现在都有方向明确的唯一 id tie-break。
3. **member 拖放导入构造守卫**：RED 首先因缺少 `acceptsFileDrops` 合同而无法编译；GREEN 用 `LibraryImportDropModifier` 的 admin 分支才构造 `.dropDestination` 与导入 overlay，member 分支只返回原内容。920/1280 根 presentation 同时断言 member=false、admin=true；Apple 全量 220 tests 通过，真实 member AX 无导入提示或管理入口。

三项作为一个逻辑一致的曲库权限修复提交：`225ede3 fix(library): 收紧权限分页与成员导入入口`。

修复后再次请求完整独立复审（`fb1dc36..225ede3`），结论为 PASS、无 Critical / Important / Minor；复审独立重跑 server/core test、Apple 220 tests + build、server clippy、三 crate fmt 与 commit-range diff check，全部通过，并确认搜索分页上限、旧响应 generation 隔离、920pt 布局和根 `PlaybackController` 单实例未回归。

早期上传的 curl `--get`/multipart 组合错误、Garage CLI 输出对齐空格未 trim 导致的 403、zsh 只读变量名 `status` 均经 systematic-debugging 定位为验收 harness 问题；修正 harness 后未改产品代码。

## 完整新鲜门禁

最终审查修复后的日志目录：`/tmp/yevune-m4-final-gates.JFZslU`。以下 13 项均为 `225ede3` 上的新鲜执行：

| 命令 | 秒 | 退出码 |
|---|---:|---:|
| `swift test --package-path clients/apple` | 1 | 0 |
| `swift build --package-path clients/apple` | 1 | 0 |
| `cargo test --manifest-path contract/Cargo.toml` | 0 | 0 |
| `cargo test --manifest-path server/Cargo.toml` | 18 | 0 |
| `cargo test --manifest-path core/Cargo.toml` | 1 | 0 |
| `cargo clippy --manifest-path contract/Cargo.toml -- -D warnings` | 0 | 0 |
| `cargo clippy --manifest-path server/Cargo.toml -- -D warnings` | 2 | 0 |
| `cargo clippy --manifest-path core/Cargo.toml -- -D warnings` | 1 | 0 |
| `cargo fmt --manifest-path contract/Cargo.toml --check` | 0 | 0 |
| `cargo fmt --manifest-path server/Cargo.toml --check` | 0 | 0 |
| `cargo fmt --manifest-path core/Cargo.toml --check` | 0 | 0 |
| `./scripts/tests/run-mac-client-test.sh` | 2 | 0 |
| `git diff --check` | 0 | 0 |

## 最终结论

状态：`DONE_WITH_CONCERNS`。真实 Garage/Rust server、130 唯一专辑、带首屏 ACL restriction 的 member offset 120、admin/member 服务端授权、920pt 大曲库、约 1275pt regular 主收藏+inspector、三类搜索、艺人详情和播放器根实例均取得真实证据；newest 稳定性与最终审查三项反馈均已 RED→GREEN 修复。最终 13/13 门禁通过。严格 1280 像素截图与两项细粒度交互仍缺足够证据，不伪称完成。
