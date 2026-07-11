# Mac 客户端一键启动设计

## 目标

在仓库根目录提供 `./scripts/run-mac-client.sh`，让开发者无需记忆 UniFFI 与 Swift Package 的构建命令即可启动 macOS 14+ 管理客户端。

## 行为

- 默认检查 `uname`、`cargo` 与 `swift`，缺失时输出中文错误并非零退出。
- 比较 core 源码、Cargo 清单、UniFFI 配置与现有 xcframework 的修改时间；产物缺失或输入更新时执行 `clients/apple/Packages/CoreFFI/scripts/build-core.sh`。
- 随后执行 `swift run --package-path clients/apple MusicApp`，并用 `exec` 将信号与退出码直接交给客户端进程。
- 传入 `--with-server` 时，先执行 `docker compose up -d`；默认不改变 Docker 状态。
- `--help` 输出用法；未知参数非零退出。

## 边界

脚本仅编排现有构建管线，不复制 core 逻辑、不生成可分发 `.app`、不处理签名/公证，也不新增依赖。

## 测试

Shell 测试以临时 PATH 中的假命令验证参数、按需构建、服务端可选启动与最终 `swift run` 参数；先观察失败，再实现脚本。README 记录一条命令和可选参数。
