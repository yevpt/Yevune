# M3 Task 6 UI 修复报告

## 状态

完成。专注播放现在接管整个主窗口内容，不再保留曲库导航、管理工具栏、任务抽屉或根 `PlayerBar`；专注页因此无法直接或间接打开 `QueuePanel`。

## TDD 记录

### RED

先扩展 `PlaybackViewPolicyTests`，覆盖：

- focus mode 隐藏 navigation、management toolbar、task drawer、root player bar 和 queue entry。
- 空队列退出 focus，并禁用 transport。
- `sliderValue` 处理 elapsed `NaN` / `±∞` / 负数，以及 duration `NaN` / `±∞` / 负数 / 零。
- 900pt 宽度阈值下的 stacked / split 布局。
- Mini 空态、buffering 和 error 呈现优先级。

`swift test --package-path clients/apple --filter PlaybackViewPolicyTests` 首次运行按预期编译失败，失败均为新 policy 接口尚未定义，无 fixture 或环境错误。

### GREEN

- 新增纯 `PlaybackViewPolicy`：窗口 chrome、focus dismiss、transport enablement、宽度布局、Mini status 与统一 finite slider clamp。
- `LibraryView` 将 focus 分支提升到 `NavigationSplitView` 及所有曲库修饰器之上；队列清空时立即返回曲库。
- `NowPlayingView` 接收默认 `.unavailable` 的 `LyricsState`，根据宽度切换 split / stacked，长标题、艺人和专辑允许约束内换行。
- NowPlaying 空队列 transport 禁用；Mini 空队列显式说明并禁用 transport / seek，同时显示克制的 buffering / error 状态。
- `PlayerBar`、`NowPlayingView` 和 `MiniPlayerView` 的播放进度 getter 全部使用同一 `sliderValue` 有限夹取策略。

## 验证

- focused policy tests：15 tests，0 failures。
- Apple 全量 tests：135 tests，0 failures。
- `swift build --package-path clients/apple`：exit 0。
- `git diff --check`：exit 0。

## 范围核对

- 保持同一 `PlaybackController` 和现有 Window scene。
- 未引入 lyrics API、新依赖、Task 7 行为或架构变更。
- `QueuePanel` 仍仅由曲库态根 `PlayerBar` 构造。

## Concerns

无阻塞 concern。本任务未启动真实服务做人工播放 smoke；边界由纯 policy tests、全量 Swift tests 和 macOS build 覆盖。
