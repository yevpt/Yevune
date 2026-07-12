# Mac 曲库管理工作台 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在一个 Mac 主窗口内完成可追踪的批量上传、详细扫描与自动曲库刷新。

**Architecture:** scanner 生成最多 500 条结构化变更，`/rest/ext/startScan` 返回详细报告；Rust core 解码并通过 UniFFI 暴露；Swift 工作流 ViewModel 只协调任务状态与刷新。上传仍由 core 按本地路径流式传输。

**Tech Stack:** Rust/axum/sqlx/reqwest/UniFFI、SwiftUI/AppKit/XCTest

## Global Constraints

- 标准 OpenSubsonic 端点保持兼容，详细扫描只走 `/rest/ext/*`。
- 音频保持流式有界读取；扫描明细最多 500 条。
- 每个行为严格 RED → GREEN；每个切片完成后提交。

---

### Task 1: 服务端详细扫描报告

**Files:**
- Modify: `server/src/scanner/mod.rs`
- Modify: `server/src/api/ext/scan.rs`
- Modify: `server/tests/ext_test.rs`

**Interfaces:**
- Produces: `ScanReport.changes: Vec<ScanChange>`、`changes_truncated: bool`
- Produces: `/rest/ext/startScan` JSON `scanResult.changes[]`

- [x] 写集成测试，断言前缀扫描返回新增曲目标题、对象键与动作，并声明 500 条上限。
- [x] 运行目标测试确认因缺少 `changes` 失败。
- [x] scanner 在写入/删除前后捕获 DTO，保留完整计数并截断明细。
- [x] 扩展 handler 序列化明细，运行 ext/scanner 测试、clippy、fmt 后提交 `feat(scanner): 返回详细扫描变更`。

### Task 2: Core/UniFFI 详细扫描与上传结果

**Files:**
- Modify: `core/src/api/scan.rs`
- Modify: `core/src/client.rs`
- Modify: `core/src/lib.rs`
- Modify: `core/tests/scan_test.rs`
- Modify: `clients/apple/Sources/Yevune/Model/LoginViewModel.swift`
- Modify: `clients/apple/Sources/Yevune/Model/CoreMusicClient.swift`

**Interfaces:**
- Produces: `DetailedScanResult`、`ScanChange`、`ScanAction`
- Produces: `MusicClient.scanPrefix(prefix: String)`
- Changes: Swift `upload(...) async throws -> Track`

- [x] 用测试 HTTP 服务写失败测试，断言 core 发送 prefix 并解码 changes。
- [x] 实现记录、枚举和门面，保留既有标准扫描 API。
- [x] 让 Swift 上传协议返回 core 已有的 Track，重新生成绑定。
- [x] 运行 core test/clippy/fmt 与 Swift 编译后提交 `feat(core): 暴露详细扫描结果`。

### Task 3: 单窗口导入与扫描工作流

**Files:**
- Create: `clients/apple/Sources/Yevune/Model/LibraryWorkflowViewModel.swift`
- Create: `clients/apple/Sources/Yevune/Views/TaskDrawerView.swift`
- Modify: `clients/apple/Sources/Yevune/Views/LibraryView.swift`
- Modify: `clients/apple/Sources/Yevune/Model/LibraryViewModel.swift`
- Modify: `clients/apple/Sources/Yevune/App.swift`
- Modify: `clients/apple/Tests/YevuneTests/LoginViewModelTests.swift`

**Interfaces:**
- Produces: `ImportTask` 逐文件状态、`ScanTask` 汇总/明细、`importFiles(_:)`、`scanLibrary()`
- Consumes: `MusicClientProviding.upload`、`scanPrefix` 与 `LibraryViewModel.load`

- [x] XCTest 先断言上传成功文字状态、批次自动扫描、失败保留和扫描后 refresh。
- [x] 运行 XCTest 确认缺少工作流类型而失败。
- [x] 实现工作流模型、底部任务抽屉、全窗口多文件 drop、工具栏文件选择和手动扫描。
- [x] 移除独立上传/扫描窗口；新增专辑 ID 以 badge 突出。
- [x] 运行 Swift 测试和一键脚本测试后提交 `feat(mac): 整合可视化曲库管理工作台`。

### Task 4: 全量与真实服务验证

**Files:**
- Modify: `README.md`
- Modify: `openapi.yaml`（若生成器输出变化）

- [x] 重建 Docker 服务端并以真实 Garage 上传音频，验证详细扫描响应。
- [x] 运行 server/core 全测试、clippy、fmt，Swift test 与 shell test。
- [x] 更新 README 操作说明并提交 `docs(mac): 补充曲库工作台使用说明`。
