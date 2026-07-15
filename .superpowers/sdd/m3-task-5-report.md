# M3 Task 5 实现报告

## 状态

已完成专辑、歌单与搜索结果的全局播放入口，接入唯一 App-level `PlaybackController`，并实现常驻底部播放栏与独立队列面板。

## 实现摘要

- `YevuneApp` 使用登录与曲库共享的 `CoreMusicClient` 构造唯一 `PlaybackController`，向曲库壳层注入。
- `PlaybackViewPolicy` 负责底栏显隐、专注页不暴露队列，以及专辑 disc/track 稳定排序。
- 专辑双击按专辑排序上下文播放；歌单双击按枚举索引播放，保留重复曲目实例；搜索双击保留服务端结果顺序。
- 专辑、歌单、搜索曲目统一提供“立即播放 / 下一首播放 / 加入队列”菜单动作。
- `PlayerBar` 包含封面与歌曲摘要、上一首/播放暂停/下一首、带本地拖动状态的进度条、随机/循环、音量/静音与队列入口。
- `QueuePanel` 按 queue instance ID 渲染，精确标记重复曲目中的当前实例，并支持播放指定实例、拖动/按钮移动、移除和清空待播。
- 任务抽屉与 PlayerBar 放入同一个 bottom safe-area inset 的纵向层级，不互相遮挡。
- 播放 UI 拆分到 `Views/Playback/`，没有把播放逻辑复制进视图，也没有实现专注页、歌词或 mini player。

## TDD 记录

1. 新增 `PlaybackViewPolicyTests` 后执行 focused test，按预期因 `PlaybackViewPolicy` 不存在而编译失败（RED）。
2. 添加最小 policy 后 focused test 3/3 通过（GREEN）。
3. 为重复曲目当前实例标记新增 Controller 测试，按预期因 `currentQueueEntryID` 不存在而编译失败（RED）。
4. Controller 发布 queue current instance ID 后该测试通过（GREEN）。
5. 独立审查指出“清空待播”启用规则不精确；新增 policy 测试并确认因 API 缺失失败（RED），实现按 current instance ID 判断后 4/4 policy tests 通过（GREEN）。

## 验证

- `swift test --package-path clients/apple --filter PlaybackViewPolicyTests`：通过，4 tests。
- `swift test --package-path clients/apple`：通过，124 tests。
- `swift build --package-path clients/apple`：通过。
- `cargo test`、`cargo clippy -- -D warnings`、`cargo fmt --check`：分别对 `server`、`core`、`contract` 三个 manifest 执行并通过（仓库根目录本身不是 Cargo workspace）。
- `git diff --check`：通过。

## 可见性冒烟

本地 debug 可执行文件成功启动并保持运行，随后正常终止。由于 SwiftPM 产物是裸可执行文件而非注册的 `.app` bundle，Computer Use / macOS AX 未能识别该窗口，因此没有完成可访问性树或截图层面的登录后布局检查；外部服务与登录数据未作为完成条件。

## Concerns

- 队列拖动依赖 macOS SwiftUI `List.onMove` 的系统交互；同时提供“上移/下移”菜单作为键盘与 VoiceOver 可操作替代。
- `openNowPlaying` 接口为后续专注页保留；本任务传入 `nil` 时摘要是非交互内容，不暴露空按钮。

## 审查修复（2026-07-16）

### RED

- 扩展 `PlaybackViewPolicyTests`，覆盖播放/缓冲展示与 VoiceOver 标签不同且均保持暂停动作、非空播放错误产生可见展示、未知时长禁用 seek、已知时长输出格式化进度 accessibility value。
- 执行 `swift test --package-path clients/apple --filter PlaybackViewPolicyTests`，按预期因 `transportPresentation`、`errorPresentation`、`canSeek` 与 `progressAccessibilityValue` 尚不存在而编译失败。

### GREEN

- `PlaybackViewPolicy` 集中提供 transport、错误、seek 与进度可访问性展示规则，未复制或修改 Controller 状态逻辑。
- `PlayerBar` 为缓冲状态显示克制的系统 `ProgressView` 与“正在缓冲”，暂停按钮保持可用且 VoiceOver 标签明确；播放错误以内联、可换行且可访问的状态显示在当前歌曲摘要旁。
- duration 未知或无效时禁用 Slider，提交拖动时再次 guard；duration 已知时 accessibility value 使用“已播放 / 总时长”格式。
- focused policy tests：8/8 通过；Swift 全量测试：128/128 通过；`swift build` 与 `git diff --check` 通过。

### Commit

- `fix(mac): 完善播放栏状态反馈`
