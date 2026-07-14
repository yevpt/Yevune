# Mac 用户与角色管理设计

- **日期**：2026-07-14
- **里程碑**：M2A — 用户与角色管理
- **平台**：macOS 14+ 原生 SwiftUI 客户端
- **状态**：设计已确认，待实现计划

## 1. 背景与目标

Mac 客户端已经具备曲库浏览、搜索、上传、扫描、标签与封面管理、多级歌单、批量曲目操作和轻量试听。长期目标是同时提供主流现代播放体验与完善曲库管理；本里程碑先交付管理员工作台的第一个独立切片：在现有主窗口内管理家庭用户与角色。

本里程碑必须继续遵守以下边界：

- 用户与角色授权由服务端强制，客户端隐藏入口不构成安全边界。
- 共享 `User`/`Role` DTO 继续来自 `contract`，Swift 不复制协议数据模型。
- HTTP 编排、OpenSubsonic 信封解析和扩展 API 调用位于 Rust `core`；Swift 只协调界面状态。
- 标准 `getUsers` 保持 OpenSubsonic 响应不变；原生客户端所需的用户 id 与自定义角色通过 `/rest/ext/getUsers` 获取。
- 不新增后端语言、数据库、缓存服务或客户端框架。

## 2. 范围

### 2.1 用户管理

- 列出全部用户，并按用户名或邮箱在本地筛选。
- 创建用户：用户名、邮箱、初始密码、是否管理员。
- 编辑用户邮箱与管理员状态。
- 为用户重置密码。
- 给用户分配或解除自定义角色。
- 删除用户，执行前二次确认。

用户名在本里程碑中不可修改。当前服务端标准 `updateUser` 以用户名定位用户且未提供重命名语义；为重命名新增自研端点不属于本切片。

### 2.2 角色管理

- 列出内建角色与自定义角色。
- 创建自定义角色。
- 删除自定义角色，执行前说明受影响的用户数量。
- 内建 `admin`、`member` 角色只读且不可删除。

角色不支持重命名。删除后重建会产生新的角色身份，也可能改变后续访问规则语义；角色重命名应与 M2B 访问控制一起另行设计。

### 2.3 明确排除

- 曲目、专辑、艺人、流派的访问规则编辑（M2B）。
- 播放队列、正在播放栏、迷你播放器和媒体键（后续播放壳层里程碑）。
- 离线下载、iOS target 和全局视觉重构。
- 修改服务端既有 OpenSubsonic 用户管理兼容行为。

## 3. 信息架构与交互

主窗口侧栏新增仅管理员可见的“管理”分区：

- “用户”进入用户主从界面。中栏显示可搜索用户列表，详情区显示账号资料、管理员开关、自定义角色选择和危险操作。
- “角色”进入角色主从界面。中栏区分内建与自定义角色，详情区显示角色类型、成员列表和删除操作。

创建用户与创建角色使用 sheet，避免占用详情选择状态。重置密码使用独立 sheet，密码只存在于表单状态中，提交结束即清空，不写日志、不进入持久化存储。

主界面沿用当前 `NavigationSplitView`，不新增设置窗口或第二套管理员仪表盘。M2B 的“访问控制”以后直接加入同一“管理”分区。

## 4. 权限识别与导航

`MusicClient::login` 在 `ping` 成功后调用标准 `getUser` 读取当前用户。只有两次请求都成功才保存会话。UniFFI `Session` 新增 `admin: bool`；Swift `SessionValue` 同步增加该字段。

`YevuneApp` 把已登录会话传给主界面，`LibraryView` 仅在 `session.admin` 为真时构造管理入口。管理员 API 即使被非管理员通过其他方式调用，服务端 `ApiAdmin` 仍会拒绝。

当前会话不落盘，因此本里程碑不增加凭证存储或自动登录。

## 5. Rust core 设计

标准 `getUsers` 不包含用户内部 id 与自定义角色，无法为既有 `assignRole(userId, roleId)` 提供可靠身份。服务端因此新增管理员专用 `/rest/ext/getUsers`：返回 `{"users":{"user":[User]}}`，其中 `User` 完整复用 `contract::User`，用户与角色 id 都使用既有不透明前缀格式。该能力通过 `getOpenSubsonicExtensions` 声明为 `userManagement` version 1，并加入 OpenAPI 路径清单。标准端点的字段与行为不改。

新增 `core/src/api/admin.rs`，独立承担用户与角色 API 编排。`MusicClient` 经 UniFFI 暴露：

- `list_users() -> Vec<User>`
- `create_user(username, email, password, admin)`
- `update_user(username, email, admin)`
- `change_password(username, password)`
- `delete_user(username)`
- `list_roles() -> Vec<Role>`
- `create_role(name) -> Role`
- `delete_role(id)`
- `assign_role(user_id, role_id)`
- `unassign_role(user_id, role_id)`

`contract::User` 与 `contract::Role` 通过 `#[uniffi::remote(Record)]` 暴露，字段保持契约定义。响应信封只在 `admin.rs` 定义必要的私有 payload，不建立重复公共 DTO。

列出带身份与自定义角色的用户走 `/rest/ext/getUsers`；创建、编辑、改密码和删除仍走标准 OpenSubsonic 用户端点并使用标准参数名；角色能力继续走既有 `/rest/ext/*`。密码作为请求参数交给既有 URL 编码器，不进入错误文本或调试日志。

## 6. Swift 状态与组件

新增单一 `AdminViewModel`，持有 `users`、`roles`、选择状态、加载状态、操作状态和可读错误。它依赖扩展后的 `MusicClientProviding`，不直接解析 HTTP。

主要视图拆分为：

- `AdminUsersView`：搜索、列表、选择与创建入口。
- `AdminUserDetailView`：邮箱、管理员状态、自定义角色、重置密码和删除。
- `AdminRolesView`：角色分组、创建与选择。
- `AdminRoleDetailView`：角色属性、成员列表与删除。
- 小型创建/密码表单按职责放在独立文件，避免管理员视图膨胀。

每次写操作成功后，`AdminViewModel` 重新并发读取用户与角色，再按 id 恢复仍存在的选择。失败时保留表单输入，展示服务端错误，不应用本地乐观变更。

## 7. 客户端安全护栏

服务端管理员鉴权是唯一授权边界；以下是防止误操作的客户端体验护栏：

- 禁止删除当前登录用户。
- 当系统只有一个管理员时，禁止移除该用户的管理员状态或删除该用户。
- 内建角色不显示删除按钮。
- 删除自定义角色前，根据 `User.roles` 统计并展示受影响用户数。
- 删除用户与角色必须二次确认；重置密码必须确认两次输入一致。
- 用户名、角色名去除首尾空白后不得为空；密码不得为空；邮箱允许为空并以空字符串调用既有服务端端点。

这些护栏不改变 OpenSubsonic 服务端语义，避免为了 UI 引入兼容性偏差。后续若要把“至少保留一个管理员”升级为全客户端一致的服务端不变量，必须单独设计并补兼容性测试。

## 8. 错误与并发

- 初次加载用户或角色任一失败时显示错误与重试按钮，不展示不完整的可编辑状态。
- 写操作期间只禁用对应表单或危险按钮，列表仍可浏览。
- 同一 ViewModel 串行执行写操作，防止快速重复点击产生乱序刷新。
- 服务端返回 40/50、无效信封或网络失败时，沿用 `CoreError` 映射；Swift 展示 `localizedDescription`。
- 刷新时对象已被其他管理员删除，清除失效选择并回到列表占位页。

## 9. TDD 与验证

### 9.1 Rust core

先写本地 mock HTTP 失败测试，覆盖：

- 登录读取当前用户并返回 `Session.admin`。
- `/rest/ext/getUsers`、`getRoles` 信封解码到共享 DTO。
- 创建/更新/删除用户的参数名和 URL 编码。
- 密码、邮箱和 Unicode 用户名编码。
- 创建/删除角色与角色分配的扩展端点参数。
- 服务端失败信封映射为 `CoreError::Server`。

### 9.2 服务端契约

先写失败集成测试，覆盖：

- `/rest/ext/getUsers` 只允许管理员访问。
- 返回用户不透明 id、邮箱、创建时间、管理员标记和全部角色名。
- `userManagement` version 1 出现在扩展发现结果中。
- 标准 `/rest/getUsers` 的 OpenSubsonic 响应保持不变。

### 9.3 Swift

使用 fake `MusicClientProviding` 先写失败测试，覆盖：

- 初次并发加载并恢复选择。
- 搜索筛选。
- 创建、编辑、重置密码、角色分配成功后刷新。
- 写失败保留状态并显示错误。
- 当前用户、最后一个管理员和内建角色护栏。
- 删除角色的受影响用户计数。

SwiftUI 结构以 `swift build`、`swift test` 和真实服务手动冒烟验证。冒烟路径为：管理员登录 → 创建普通用户 → 创建自定义角色 → 分配角色 → 修改邮箱与密码 → 解除并删除角色 → 删除测试用户；普通用户重新登录后看不到管理入口且直接调用管理 API 被服务端拒绝。

### 9.4 完成门槛

- `cargo test`
- `cargo clippy -- -D warnings`
- `cargo fmt --check`
- `swift build --package-path clients/apple`
- `swift test --package-path clients/apple`
- 一键启动脚本测试与上述真实服务冒烟

## 10. 后续路线

M2A 完成后，按长期目标依次推进：

1. M2B 访问控制规则编辑，把曲目/专辑/艺人/流派范围与用户/角色允许名单接入管理员分区。
2. 现代播放壳层：全局播放队列、底部正在播放栏、播放控制、进度与音量、系统媒体键和迷你播放器。
3. 曲库体验迭代：更完整的艺人/专辑导航、收藏与最近播放、队列入口、现代搜索和大曲库分页。
4. 离线与跨 Apple 平台能力在 core 状态机稳定后另开设计循环。
