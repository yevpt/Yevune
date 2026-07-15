# M3 Task 7 系统媒体与会话清理报告

## 状态

已完成 Task 7 的系统媒体命令、Now Playing 元数据、认证封面异步加载、登出清理与生命周期隔离。自动化门禁全部通过；真实 macOS 播放 smoke 因本机 Docker daemon 未运行而未执行，阻塞证据见下文。

## TDD 记录

### RED 1：缺失边界

先新增 `SystemMediaCoordinatorTests`，并在现有测试类中加入 logout 与 artwork 测试，再运行：

- `swift test --package-path clients/apple --filter SystemMediaCoordinatorTests`：exit 1；缺失 `SystemMediaCoordinator`、`RemoteCommandSurface`、`NowPlayingSurface`。
- `swift test --package-path clients/apple --filter LoginViewModelTests`：exit 1；`LoginViewModel` 缺失 `logout()`。
- `swift test --package-path clients/apple --filter PlaybackControllerTests/testControllerLoadsArtworkForCurrentSystemMetadata`：exit 1；缺失 artwork/system-media 注入接口。

失败均由待实现能力缺失导致，不是 fixture 或环境错误。

### RED 2：生命周期缺口

扩展 Controller 回归测试后运行 `swift test --package-path clients/apple --filter PlaybackControllerTests`，38 tests 中 4 个断言按预期失败：

- 首次发布的系统 duration 为 `0` 而不是曲目时长 `180`。
- 同一播放生命周期重复注册系统 handlers。
- shutdown 后 stale seek 仍能触达 engine。

修复后该轮 38/38 通过。

### RED 3：跨重新登录的旧命令

继续扩展 stale handler 测试，覆盖 shutdown 后重新播放再收到旧 play/pause/next/seek：focused test 4 个断言失败，旧命令会推进新队列并触达 engine。

加入 system-media generation token 后 focused test 转绿；旧生命周期 handler 即使在新播放已开始后晚到也被拒绝。

### GREEN

- `SystemMediaCoordinator` 通过可注入 surface 映射 play、pause、previous、next 与 change-position；同一生命周期只安装一次，clear 移除全部 targets 并清空 Now Playing。
- Now Playing 发布 title、artist、album、duration、elapsed、rate 与 `MPMediaItemArtwork`。
- `URLSessionPlaybackArtworkLoader` 只接受 HTTP 200 并解码 `NSImage`，失败路径不记录带认证参数的 URL。
- Controller 使用 queue instance ID、load generation、cover URL 与 task cancellation 四重 gate；切歌或 shutdown 后的旧封面不能覆盖当前封面。
- Controller shutdown 停止 engine，清 queue/current media/timing/error/retry，取消 transition/artwork task，移除 engine callback，清系统信息与命令，并隔离旧 remote handler generation；显式重新播放会重新注册新生命周期 handlers。
- `LoginViewModel.logout()` 清 password/session/error；用户菜单明确提供“退出登录”，App closure 严格先调用 `playback.shutdown()` 再调用 `login.logout()`。

## 验证命令

### Swift

- `swift test --package-path clients/apple --filter SystemMediaCoordinatorTests`：5 tests，0 failures。
- `swift test --package-path clients/apple --filter LoginViewModelTests`：12 tests，0 failures。
- `swift test --package-path clients/apple --filter PlaybackControllerTests`：39 tests，0 failures。
- `swift test --package-path clients/apple`：147 tests，0 failures。
- `swift build --package-path clients/apple`：exit 0。
- `for i in {1..20}; do swift test --package-path clients/apple --filter PlaybackControllerTests ...; done`：20/20 轮，每轮 39 tests、0 failures。

### 仓库门禁

- `cargo test --manifest-path contract/Cargo.toml`：exit 0。
- `cargo test --manifest-path server/Cargo.toml`：exit 0。
- `cargo test --manifest-path core/Cargo.toml`：exit 0。
- `cargo clippy --manifest-path contract/Cargo.toml -- -D warnings`：exit 0，无 warning。
- `cargo clippy --manifest-path server/Cargo.toml -- -D warnings`：exit 0，无 warning。
- `cargo clippy --manifest-path core/Cargo.toml -- -D warnings`：exit 0，无 warning。
- `cargo fmt --manifest-path contract/Cargo.toml --check`：exit 0。
- `cargo fmt --manifest-path server/Cargo.toml --check`：exit 0。
- `cargo fmt --manifest-path core/Cargo.toml --check`：exit 0。
- `./scripts/tests/run-mac-client-test.sh`：exit 0，输出 `run-mac-client tests: PASS`。
- `git diff --check`：exit 0。

## 全分支自审

按 `docs/superpowers/specs/2026-07-15-mac-playback-shell-design.md` 逐节复核播放队列、Engine、Controller、三处播放 UI、系统媒体、失败恢复与登出生命周期；未发现未修复的 Critical 或 Important 问题。额外发现并用 RED/GREEN 修复了“旧 remote handler 在重新播放后晚到”的跨生命周期竞态。

## 真实 macOS smoke 与未验证项

执行 `./scripts/run-mac-client.sh --with-server`，exit 1。准确阻塞信息：

```text
unable to get image 'dxflrs/garage:v2.3.0': failed to connect to the docker API at unix:///Users/vpt/.orbstack/run/docker.sock: connect: no such file or directory
```

同时 Compose 报告 `GARAGE_ACCESS_KEY`、`GARAGE_SECRET_KEY`、`YEVUNE_APP_SECRET` 未设置。由于服务端与真实音频数据无法启动，以下八项均未声称已验证：专辑中段顺序推进、重复曲目歌单实例顺序、跨页面播放、底栏/队列控制、专注页无队列、迷你播放器状态连续、硬件媒体键与系统元数据、登出后声音和系统信息消失。没有可登录的 app 状态，因此未使用 Computer Use 伪造 UI 证据。

## 提交

- `feat(mac): 接入系统播放控制与会话清理`

## Concerns

- 唯一未闭环项为真实服务端/真实音频 smoke；需要启动 Docker daemon、提供 Compose 必需密钥和可登录数据后按 brief 的八项清单人工验证。
- 自动化已覆盖重复注册、旧 engine 事件、旧/晚到 artwork、shutdown 后 remote command、重新播放后的旧 handler、重新注册与显式恢复播放。

## 独立复审修复（2026-07-16）

### Important 1：登出清除 Rust core 会话

#### RED

- 新增 `logout_clears_authenticated_session`，运行 `cargo test --manifest-path core/Cargo.toml --test login_test logout_clears_authenticated_session`，按预期因 `MusicClient.logout` 不存在而编译失败。
- 加入初始 clear API 后新增并发测试 `logout_invalidates_an_earlier_in_flight_login`；focused test 按预期失败，证明旧实现会让 logout 前开始、logout 后完成的 login 重新写回认证会话。
- Swift `LoginViewModelTests` 新增 core 调用与晚到 login 测试；首次运行 13 tests 出现 3 个预期行为失败：未调用 client logout，且旧 login 会恢复 session。

#### GREEN

- Rust `MusicClient` 新增 UniFFI async `logout()`，清空 `RwLock<Option<AuthenticatedSession>>`，从 core 持有状态中释放明文密码。
- `AtomicU64` session generation 在 logout 时失效旧登录尝试；登录写回在持有 session write lock 时再次核对 generation，关闭检查与写入之间的竞态。
- `MusicClientProviding` 将 logout 设为强制 async 协议要求，`CoreMusicClient` 转发至 UniFFI，测试 fake 显式实现。
- `LoginViewModel` 使用 generation 拒绝旧 submit 的晚到结果；先等待 core clear 完成，再发布 password/session/error 清空。顺序测试在 core logout suspended 时确认 Swift session/password 尚未提前清除。
- App 的登出 closure 在创建 async logout Task 前同步执行 `playback.shutdown()`。

### Important 2：认证封面不进入共享缓存

#### RED

- 新增 `PlaybackArtworkLoaderTests.testAuthenticatedArtworkUsesEphemeralNoCachePolicy`；focused test 按预期因配置与请求 policy API 不存在而编译失败。

#### GREEN

- Loader 不再使用 `URLSession.shared`，改用独立 `URLSessionConfiguration.ephemeral` session。
- 显式设置 `urlCache=nil`、`httpCookieStorage=nil`、`urlCredentialStorage=nil`、禁止 cookie 接受/写入，并将 configuration 与每个 `URLRequest` 的 cache policy 设为 `reloadIgnoringLocalCacheData`。
- 测试直接核对 configuration 与 request policy，认证 URL 仍不进入日志。

### 复审后验证

- `cargo test --manifest-path core/Cargo.toml --test login_test`：4 tests，0 failures。
- `cargo test --manifest-path core/Cargo.toml`：exit 0。
- `cargo clippy --manifest-path core/Cargo.toml -- -D warnings`：exit 0，无 warning。
- `cargo fmt --manifest-path core/Cargo.toml --check`：exit 0。
- `swift test --package-path clients/apple --filter LoginViewModelTests`：14 tests，0 failures。
- `swift test --package-path clients/apple --filter PlaybackArtworkLoaderTests`：1 test，0 failures。
- `swift test --package-path clients/apple`：150 tests，0 failures。
- `swift build --package-path clients/apple`：exit 0。
- contract/server 的 test、clippy `-D warnings`、fmt check：全部 exit 0。
- `./scripts/tests/run-mac-client-test.sh`：输出 `run-mac-client tests: PASS`。
- `git diff --check`：exit 0。

### 复审修复提交

- `fix(mac): 清理登出凭证与封面缓存`
