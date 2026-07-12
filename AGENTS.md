# AGENTS.md — 项目宪法（所有 AI 编码工具必读）

> 本文件是 Claude Code、Codex、Cursor 等**所有** AI 编码工具在本仓库工作时的最高约束。
> 开始任何编码前**必须读完本文件**。与本文件冲突的做法一律不被接受。
> 权威设计见 [`docs/superpowers/specs/2026-07-10-yevune-server-design.md`](docs/superpowers/specs/2026-07-10-yevune-server-design.md)，决策理由见 [`docs/adr/`](docs/adr/)。

---

## 1. 项目是什么

自托管的音乐流媒体服务（类 Navidrome），面向**个人 + 家庭**，不对外。多平台客户端 + Rust 服务端 + Garage(S3) 存储。核心诉求：**服务端省内存高性能、客户端原生流畅、接口全平台复用、部署对小白友好**。

---

## 2. 架构不可变量（红线，禁止擅自更改）

违反以下任一条 = 立即停止，先更新 spec + 写 ADR 说明理由，经人工确认后才能动。

1. **服务端语言是 Rust**。不引入其他后端语言。
2. **存储**：Garage(S3) 是音频文件的唯一源。**SQLite 索引放服务器本地磁盘**（严禁把 DB 放对象存储）；**转码缓存放 Garage**。
3. **数据库是 SQLite**（经 `sqlx`）。**禁止引入 Postgres、Redis 或任何独立数据库/缓存服务**——见 [ADR-0002]。写负载极小，WAL 模式足矣。留 `sqlx` 抽象作为未来迁移后路。
4. **API = OpenSubsonic 兼容子集 + 命名空间隔离的自研扩展**。**禁止破坏 OpenSubsonic 兼容性**（现成客户端 Amperfy 等必须始终能用）。扩展一律走 `/rest/ext/*`，并通过 `getOpenSubsonicExtensions` 声明。
5. **跨平台复用靠 Rust**：共享类型在 `contract` crate，共享客户端逻辑在 `core` crate（UniFFI 生成绑定）。**禁止在各平台重复实现核心逻辑**。UI 才是平台专属。
6. **客户端 UI 必须原生**（SwiftUI / Compose / …）。**禁止 Electron / Flutter / React Native** 作为主客户端方案。
7. **多用户 + 曲库访问控制**：家庭多用户，每用户独立歌单空间；曲库默认对所有人开放，管理员可按 曲目/专辑/艺人/流派 作用域限制。**授权在服务端强制，客户端不可绕过**。
8. **转码：按需 + 缓存，不预转码**。**绝不缓存不完整的转码产物**（客户端中途断开必须丢弃临时文件）。
9. **不强制 HTTPS**：默认支持明文 HTTP（局域网小白友好），TLS/反代为进阶可选。

---

## 3. 技术栈（已锁定，勿替换）

| 用途 | 选型 |
|---|---|
| HTTP 框架 | `axum`（tokio） |
| 数据库 | SQLite via `sqlx`（编译期查询校验、迁移） |
| 对象存储 | `object_store` 或 `aws-sdk-s3`（二选一，首任务定，后续勿换） |
| 读音频标签 | `symphonia` / `lofty`（纯 Rust） |
| 转码 | FFmpeg 子进程 |
| 全文搜索 | SQLite FTS5 |
| 日志 | `tracing` 结构化日志 |
| 跨语言绑定 | UniFFI |

如需引入**任何新依赖**：先确认标准库/已选库无法胜任，在 PR/提交说明中给出理由。能不加就不加（YAGNI）。

---

## 4. 仓库布局

```
Yevune/
├── AGENTS.md / CLAUDE.md        # 本宪法（Claude 版引用本文件）
├── .agents/skills/              # AI skills（权威）；.claude/.cursor 下为符号链接
├── .githooks/                   # 版本化 git hooks（core.hooksPath）
├── scripts/                     # hook 校验脚本等
├── server/          Rust 服务端 (axum)；模块：api/index/storage/scanner/transcode
├── contract/        Rust 共享 DTO → 服务端+客户端共用，生成 OpenAPI
├── core/            Rust 客户端核心 → UniFFI 生成各语言绑定
├── clients/apple|web|android|desktop
└── docs/
    ├── superpowers/specs/       设计文档（权威）
    ├── superpowers/plans/       实现计划 + 各任务交接提示词
    └── adr/                     架构决策记录（为什么这么定）
```

**边界原则**：文件小而专注、一个职责；一起变化的代码放一起；按职责拆分而非技术分层。文件变臃肿是"它做太多"的信号。

---

## 5. 工作流（强制）

1. **TDD**：先写失败测试 → 跑红 → 最小实现 → 跑绿 → 提交。禁止无测试提交产品代码。
2. **契约先行**：改跨端接口，先改 `contract`，靠编译错误驱动两端同步更新。
3. **小步频繁提交**。写/改 commit 前**必须**先读并遵守 [`.agents/skills/git-commit/SKILL.md`](.agents/skills/git-commit/SKILL.md)。身份与 message 由 `.githooks` 硬校验，不合规直接拒。
4. **改架构前先改文档**：任何偏离 spec 的设计，先更新 spec 并写 ADR，不得"边写边悄悄改架构"。
5. **CI 必须绿**：`cargo test`、`cargo clippy -- -D warnings`、`cargo fmt --check` 全过才算完成。
6. **任务边界**：只做当前任务范围内的事。发现范围外的问题 → 记录到 issue/TODO，不擅自扩大改动。
7. **资源守护**：转码/传输**流式、有界缓冲，绝不把整个音频文件读进内存**；转码/扫描并发用信号量限流。

### 5.1 Git hooks 启用（克隆后一次）

```bash
git config core.hooksPath .githooks
# 或：./scripts/setup-git-hooks.sh
```

未设置则 hooks 不生效；AI 与人工提交均须启用。

---

## 6. 完成的定义（每个任务通用 DoD）

- [ ] 该任务的测试全部通过（含失败→通过的 TDD 记录）
- [ ] `cargo clippy -- -D warnings` 与 `cargo fmt --check` 无报错
- [ ] 未引入红线禁止的依赖/服务
- [ ] 未破坏 OpenSubsonic 兼容性（若涉及 API）
- [ ] 公共接口有文档注释；若改了跨端类型，`contract` 与消费端同步
- [ ] 提交信息符合规范，改动局限在任务范围内

---

## 7. 各工具如何使用本文件

- **Claude Code**：根 `CLAUDE.md` 已 `@AGENTS.md` 引用本文件，自动加载；skills 见 `.claude/skills/`（链到 `.agents/skills/`）。
- **Codex**：原生读取 `AGENTS.md`；skills 见 `.agents/skills/`。
- **Cursor**：读取 `AGENTS.md`；skills 见 `.cursor/skills/`（链到 `.agents/skills/`）。
- **其他 AI**：交接提示词会显式要求"先阅读并遵守 AGENTS.md 与对应 spec"。

### 按场景读 skill（`.agents/skills/<name>/SKILL.md`）

- 写 commit message / 执行 `git commit` → `git-commit`

**当你（AI）不确定某个决策时**：查 spec 和 ADR；仍不明确 → 停下来问人，不要臆测发挥。
