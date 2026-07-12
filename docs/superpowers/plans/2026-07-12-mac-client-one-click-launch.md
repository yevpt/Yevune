# Mac 客户端一键启动 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 用一条根目录命令按需构建 UniFFI 产物并启动 Mac 客户端。

**Architecture:** POSIX shell 脚本只负责编排现有 `build-core.sh` 与 Swift Package。独立 shell 测试通过临时假命令观察调用，不启动真实 GUI。

**Tech Stack:** POSIX shell、Swift Package Manager、现有 Rust/UniFFI 构建脚本

## Global Constraints

- macOS 14+；不新增依赖。
- 默认不改变 Docker 状态，只有 `--with-server` 执行 `docker compose up -d`。
- core 输入比 xcframework 新或产物缺失时才重建。

---

### Task 1: 一键启动编排

**Files:**
- Create: `scripts/run-mac-client.sh`
- Create: `scripts/tests/run-mac-client-test.sh`
- Modify: `README.md`

**Interfaces:**
- Consumes: `clients/apple/Packages/YevuneCoreFFI/scripts/build-core.sh`
- Produces: `scripts/run-mac-client.sh [--with-server|--help]`

- [x] **Step 1: Write the failing shell test**

测试在临时目录放置 `uname`、`cargo`、`swift`、`docker` 假命令，把调用写入日志；断言默认运行 `swift run --package-path clients/apple Yevune`，`--with-server` 先调用 `docker compose up -d`，未知参数失败，并通过设置输入/产物时间验证按需调用 `build-core.sh`。

- [x] **Step 2: Run test to verify it fails**

Run: `sh scripts/tests/run-mac-client-test.sh`
Expected: FAIL，因为 `scripts/run-mac-client.sh` 不存在。

- [x] **Step 3: Write minimal launcher**

脚本解析两个公开参数、检查 macOS/cargo/swift（以及可选 docker）、以 `find ... -newer` 判断绑定是否过期、按需调用现有构建脚本，最后 `exec swift run --package-path clients/apple Yevune`。

- [x] **Step 4: Run automated and real verification**

Run: `sh scripts/tests/run-mac-client-test.sh`
Expected: PASS。

Run: `./scripts/run-mac-client.sh --help`
Expected: 输出用法且退出 0。

Run: `swift test --package-path clients/apple`
Expected: 全部 XCTest 通过。

- [x] **Step 5: Document and commit**

README 将启动步骤改为 `./scripts/run-mac-client.sh`，并记录 `--with-server`。提交信息：`feat(mac): 支持一键启动客户端`。
