# Yevune 品牌迁移设计

- **日期**：2026-07-12
- **状态**：用户已确认方案，待书面规格复核

## 目标

把本仓库及其可部署产物从 `music`/`MusicApp` 完整迁移为 **Yevune**。这是一次破坏性品牌迁移；现有测试数据、对象存储 bucket、SQLite 文件和环境变量不需要兼容。

## 命名映射

| 范围 | 旧名称 | 新名称 |
|---|---|---|
| 仓库目录 | `music` | `Yevune` |
| 服务端 Cargo 包/二进制/库 | `music-server` / `music_server` | `yevune-server` / `yevune_server` |
| 客户端核心 Cargo 包/库 | `music-core` / `music_core` | `yevune-core` / `yevune_core` |
| UniFFI Swift 模块与产物 | `CoreFFI` / `MusicCoreFFI` | `YevuneCoreFFI` |
| Apple Swift 包、可执行 Target、App 类型、测试 Target | `MusicApp` / `MusicAppTests` | `Yevune` / `YevuneTests` |
| Apple FFI 目录 | `Packages/CoreFFI` | `Packages/YevuneCoreFFI` |
| Docker Compose 服务、volume、环境变量、SQLite 路径 | `server`、`server-data`、`MUSIC_*`、`music.sqlite` | `yevune`、`yevune-data`、`YEVUNE_*`、`yevune.sqlite` |
| Garage bucket 与测试对象键前缀 | `music` / `music/` | `yevune` / `library/` |
| OpenSubsonic 响应 `type` | `music-server` | `yevune-server` |

OpenSubsonic 端点、响应结构、认证参数和扩展命名空间保持不变；品牌迁移不得破坏协议兼容性。

## 实施边界

1. 重命名 Rust 包、库 crate 引用、锁文件、Docker 构建参数与测试断言。
2. 重命名 Apple SwiftPM target/目录、Swift `import`、应用入口和 FFI 生成/打包脚本；重新生成 bindings 与 xcframework。
3. 用 `YEVUNE` 环境变量前缀、`yevune` bucket 和 `library/` 正式对象键更新服务端默认配置、Compose、示例、测试夹具及部署文档。
4. 更新 README、权威规格、ADR 和实施计划中的项目专名、路径及运行命令；保留普通的“音乐”领域描述。
5. 从父目录把仓库目录移动到 `/Users/vpt/Documents/Codes/Yevune`，并在新目录重设 hooks 路径。

## 验证

- 以新名称运行 `cargo test`、`cargo clippy -- -D warnings`、`cargo fmt --check`（server、core、contract）。
- 生成 Yevune FFI 产物后运行 `swift test` 与 `swift run --package-path clients/apple Yevune --help`。
- 运行部署和脚本测试，确认 Dockerfile 构建 `yevune-server`，Compose 使用 `YEVUNE_*`、`yevune` bucket，且一键启动脚本运行 `Yevune`。
- 执行全仓库定向搜索，确保没有遗留旧的项目/构建/配置标识；`music` 作为自然语言领域词汇不构成失败。

## 风险与处置

新环境变量、bucket、对象键与 SQLite 路径会使旧部署无法直接读取数据。用户已确认测试数据可丢弃；不提供迁移或兼容层。目录移动发生在所有 Git 提交与验证完成后，以避免在操作期间失去工作目录。
