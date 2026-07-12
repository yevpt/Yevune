# 自托管家庭音乐服务

面向个人 + 家庭的自托管音乐流媒体服务（类 Navidrome）：Rust 服务端 + Garage(S3) 存储，
**OpenSubsonic 兼容**——现成客户端（Amperfy、play:Sub 等）可直接连。多用户、每人独立歌单空间、
管理员可按曲目/专辑/艺人/流派限制曲库可见性。默认明文 HTTP，局域网开箱即用。

> 权威设计见 [`docs/superpowers/specs/2026-07-10-music-server-design.md`](docs/superpowers/specs/2026-07-10-music-server-design.md)，
> 编码约束见 [`AGENTS.md`](AGENTS.md)。

## 特性

- **OpenSubsonic 兼容子集**：浏览 / 搜索(FTS5) / 歌单 / 流(stream) / 收藏 / 扫描 / 用户管理。
- **自研扩展**（`/rest/ext/*`）：多级歌单树、曲库上传/改标签、访问控制、角色管理。
- **按需转码 + 缓存**：FLAC/ALAC → AAC/Opus，流式有界缓冲，绝不缓存不完整产物。
- **曲库访问控制**：默认全开放，管理员按作用域限制；授权在服务端强制，客户端不可绕过。
- **省内存单二进制**：musl 静态编译，唯一外部依赖 FFmpeg（已打包进镜像）。

## 快速上手（Docker Compose）

前置：已装 Docker 与 Docker Compose。

```bash
# 1) 准备环境变量
cp .env.example .env
# 编辑 .env：设 MUSIC_APP_SECRET（如 `openssl rand -hex 32`）

# 2) 先起 Garage 对象存储
docker compose up -d garage

# 3) 初始化 Garage（建集群布局 + bucket + 访问密钥）
#    获取 node id 并应用布局：
docker compose exec garage /garage status
docker compose exec garage /garage layout assign -z dc1 -c 1G <NODE_ID>
docker compose exec garage /garage layout apply --version 1
#    建 bucket 并签发密钥：
docker compose exec garage /garage bucket create music
docker compose exec garage /garage key create music-key
#    把上一步输出的 Key ID / Secret 写入 .env 的 GARAGE_ACCESS_KEY / GARAGE_SECRET_KEY，并授权：
docker compose exec garage /garage bucket allow --read --write music --key music-key

# 4) 起服务端（首次会构建 musl 静态镜像）
docker compose up -d server

# 5) 查看首启管理员密码（未在 .env 指定 MUSIC_ADMIN_PASSWORD 时自动生成）
docker compose logs server | grep 首启
```

服务默认监听 `http://<主机>:4533`。健康检查：`curl http://localhost:4533/healthz`；
OpenSubsonic 探测：`curl "http://localhost:4533/rest/ping?u=admin&p=<密码>&v=1.16.1&c=app"`。

### 连接客户端

任一 OpenSubsonic 客户端（Amperfy / play:Sub / Symfonium…）填：

- 服务器：`http://<主机>:4533`
- 用户名 / 密码：管理员账号（或管理员后续创建的家庭成员账号）

### Mac 管理客户端（M1 开发）

前置：Rust stable、Xcode（含命令行工具）与 macOS 14+。在仓库根目录一键启动：

```bash
./scripts/run-mac-client.sh
```

需要同时启动 Docker 服务端时使用 `./scripts/run-mac-client.sh --with-server`。脚本会在 Rust core 或 UniFFI 输入更新后自动重建绑定，其他时候直接启动客户端。构建脚本固定 `MACOSX_DEPLOYMENT_TARGET=14.0`，当前生成 Apple Silicon (`arm64`) 框架；框架与生成的 Swift 源码是本地构建产物，不提交。

M1 客户端支持登录、浏览与搜索、拖拽上传、标签覆盖编辑、删除/移动、扫描状态、封面显示/替换和 AVFoundation 流式试听。上传与封面替换只把本地路径交给 Rust core，以有界流式请求传输；客户端不持有 Garage 凭证。

登录后可把多个音频文件拖到主窗口任意位置，或点击工具栏“导入音乐”。底部任务抽屉逐项显示上传进度与成功/失败结果；上传结束后自动扫描并刷新左侧专辑列表。工具栏“扫描曲库”用于手动全库扫描，任务抽屉会展示新增、更新、删除汇总及具体曲目。

### 导入音乐

把音频文件放进 Garage 的 `music` bucket（S3 客户端上传，或用扩展接口 `uploadTrack`），
然后触发扫描：`curl "http://localhost:4533/rest/startScan?u=admin&p=<密码>&v=1.16.1&c=app"`。

## 配置

单一 TOML（`config.toml`）+ 环境变量覆盖（前缀 `MUSIC`，分隔符 `__`，如
`MUSIC__SERVER__PORT=8080`）。全部字段带合理默认，详见 [`server/src/config.rs`](server/src/config.rs)。
密钥 `MUSIC_APP_SECRET` 用于密码加密与会话签名，必须设置。

## 进阶：TLS / 反向代理

默认明文 HTTP（局域网小白友好）。需要 HTTPS 时在前面加反向代理（Caddy/Nginx）或配置
`[tls]` 证书路径即可，OpenSubsonic 客户端改用 `https://` 连接。

## 本地开发

```bash
cd server && cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check
```

重新生成 OpenAPI（供 web 端生成 TS 类型）：`cd server && cargo run --bin gen_openapi`
（写出仓库根 [`openapi.yaml`](openapi.yaml)）。
