# macOS 同步歌词设计

- **日期**：2026-07-18
- **子项目**：Yevune macOS 原生客户端
- **状态**：依据既有专注播放页方向确认

## 1. 目标

把 M3 专注播放页的歌词占位状态升级为真实歌词体验：当前歌曲变化时自动加载歌词；带 LRC 时间戳的歌词跟随播放进度高亮并自动滚动；纯文本歌词可自然阅读；没有歌词或请求失败时显示安静、明确且可访问的状态。专注页仍只服务当前歌曲，不放回播放队列或“接下来播放”。

## 2. 协议与边界

- 使用 OpenSubsonic `getLyricsBySongId` 与 `songLyrics` version 1，不新增 `/rest/ext/*` 私有歌词协议。
- `getOpenSubsonicExtensions` 声明 `songLyrics: [1]`。
- 跨端 DTO 先进入 `contract`，`core` 负责认证请求和响应解码，Swift 只负责状态编排与原生呈现。
- 服务端从 Garage 原始音频的头部标签按需读取歌词；不把整个音频读入内存，不新增数据库或缓存服务。
- 第一版支持 Lofty 可映射的 `LYRICS` 与 `UNSYNCEDLYRICS` 文本。`LYRICS` 若包含 LRC 时间戳则输出同步歌词，否则按非同步歌词输出。
- 第一版不支持 ID3 `SYLT` 二进制帧、逐词卡拉 OK、在线歌词供应商或外置 `.lrc` 文件；这些能力不得阻塞基础逐行同步链路。

## 3. 契约

共享类型：

- `LyricLine { start: UInt64?, value: String }`，`start` 单位为毫秒。
- `StructuredLyrics { displayArtist?, displayTitle?, lang?, offset, synced, lines }`。

服务端 JSON 继续使用 OpenSubsonic 字段名：`lyricsList.structuredLyrics[].line[]`。不存在歌词时返回成功信封和空 `structuredLyrics`，不把“无歌词”当成协议错误。

## 4. 服务端解析

请求流程：

1. 认证用户并解析不透明曲目 ID。
2. 复用现有可见性规则查询媒体源；无权限与不存在统一返回 not found。
3. 只读取 `min(object_size, HEADER_READ_CAP)` 字节并用现有 Lofty 标签解析器读取歌词。
4. 优先使用 `ItemKey::Lyrics`，回退 `ItemKey::UnsyncLyrics`。
5. LRC 解析支持 `[mm:ss]`、`[mm:ss.xx]`、`[mm:ss.xxx]`、一行多个时间戳和 `[offset:+/-N]`；元数据标签不作为歌词行。
6. 同步行按时间稳定排序；同一时间的原始顺序保持不变。没有任何有效时间戳则按换行生成非同步歌词。

歌词文本受 256 KiB 独立上限约束；异常超大标签不会进入响应，端点按“无歌词”兼容处理，不允许无界分配。

## 5. core 与 Apple 状态

`MusicClient.getLyricsBySongId(id:)` 返回 `[StructuredLyrics]`。Apple 侧增加独立 `LyricsViewModel`：

- 当前 track ID 改变时取消旧请求并进入 loading。
- 只接受当前请求对应的返回结果，旧歌曲晚到结果不能覆盖新歌曲。
- 优先选择第一份同步歌词，否则选择第一份非同步歌词。
- 同步歌词根据 `playback.elapsed * 1000 - offset` 选择最后一个 `start <= 当前时间` 的行。
- 登出或队列清空时清除歌词状态。

## 6. macOS 界面

- 同步歌词使用 `ScrollViewReader`，当前行采用主文本色与较高字重，其他行降低强调。
- 当前行变化时自动滚动到舞台中部；开启 Reduce Motion 时禁用滚动动画。
- 用户可以手动滚动阅读；第一版不实现点击歌词跳转。
- 非同步歌词按段落/行呈现，不压成单个大文本块。
- loading、无歌词和失败状态使用中文文案与 VoiceOver 标签。
- 920pt stacked 与 1180pt+ split 布局均不得让歌词与底部 transport 冲突。

## 7. 测试与验收

- Rust：LRC 各时间格式、offset、多时间戳、纯文本、空歌词、超限；端点认证、权限、空结果和标准 JSON；扩展声明。
- core：认证参数、标准响应解码、空数组与错误信封。
- Swift：请求竞态、同步/非同步选择、offset 当前行计算、曲目切换清理、Reduce Motion 呈现策略。
- 真实 macOS：带 LRC 与纯文本歌词各一首，在 920pt 与宽窗口播放；观察自动滚动、上一首直接切歌后歌词切换、返回曲库不断播、无歌词状态和普通成员权限路径。

完成前运行 Swift 全量测试、三个 Rust crate 的测试/Clippy/fmt、mac 启动脚本和真实界面验收。
