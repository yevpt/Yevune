# Yevune macOS M2B 访问控制设计

- **日期**：2026-07-14
- **里程碑**：M2B — 曲库访问控制
- **前置里程碑**：M2A 用户与角色管理
- **状态**：已批准（用户授权按产品判断持续推进）

## 1. 目标

在 macOS 原生客户端中交付可日常使用的家庭曲库可见范围管理：管理员既能集中审计全部限制规则，也能从专辑、艺人、曲目和流派上下文直接设置可见成员与角色。所有授权仍由服务端强制，客户端只负责管理和解释规则。

本里程碑必须继续遵守：

- 没有规则即全家可见；只有受限内容才存规则。
- 规则是允许名单，不引入 deny 名单。
- 曲目 > 专辑 > 艺人 > 流派，最具体规则优先。
- 管理员始终可见全部内容，客户端不可改变该事实。
- 管理 API 继续使用 `/rest/ext/*`，不改变 OpenSubsonic 标准端点。
- 跨端 DTO 先改 `contract`，请求逻辑进入 `core`，SwiftUI 只实现平台界面。

## 2. 方案比较与选择

### 方案 A：只做集中规则表

集中页便于审计、筛选和删除规则，实现成本最低；但管理员在浏览具体音乐时需要记住名称并跳转，日常操作路径割裂。

### 方案 B：只做对象上下文入口

从当前专辑、曲目或流派直接编辑最自然；但无法快速回答“目前哪些内容受限”，也难以发现过期规则。

### 方案 C：双入口共享编辑器（采用）

“管理 → 访问控制”负责全局审计；曲库对象的菜单负责就地设置。两个入口都构造同一个 `AccessScopeTarget`，打开同一个 `AccessRuleEditorView`，并调用同一个 `AccessControlViewModel`。这样不复制权限逻辑，且同时覆盖管理与日常浏览场景。

## 3. 用户语义

界面避免直接暴露 ACL 术语，统一使用“可见范围”：

- 无规则：显示“全家可见”。
- 有一个或多个授权主体：显示“仅所选成员和角色可见”。
- 有规则但允许名单为空：显示“仅管理员可见”，保存前二次确认。
- 删除规则：文案为“恢复全家可见”，而不是“删除 ACL”。
- `admin` 用户和内建 `admin` 角色不进入可选允许名单；界面说明管理员始终可见。
- 上层规则可能被更具体规则覆盖。编辑器固定展示优先级说明，但首版不制作逐曲有效权限模拟器。

规则目标不可在编辑时更换。若要限制另一个对象，创建新规则；这能避免一次保存隐式删除旧目标的规则。

## 4. 契约与服务端

### 4.1 共享 DTO

沿用 `ScopeType`、`PrincipalType`、`Principal` 和 `AccessRule`。为集中规则列表增加可选展示名：

```rust
pub struct AccessRule {
    pub id: String,
    pub scope_type: ScopeType,
    pub scope_id: String,
    pub scope_name: Option<String>,
    pub grants: Vec<Principal>,
}
```

`scope_name` 只用于展示：曲目标题、专辑名、艺人名或流派名。权限判定仍只使用 `scope_type + scope_id`。字段为可选是为了兼容已删除或尚未解析的目标；JSON 使用 camelCase `scopeName`。

### 4.2 规则响应补全

`setAccessRule` 与 `getAccessRules` 返回规则前，由服务端按作用域查询展示名。目标已不存在时返回 `scopeName: null`，集中页显示“对象已不存在”并允许管理员恢复全家可见。

不新增数据库、缓存或新服务。规则数量属于家庭管理数据，按规则查询展示名足够；后续只有实测证明需要时才批量优化 SQL。

### 4.3 主体删除一致性

`access_rule_grants` 是多态引用，不能用单一外键同时指向用户与角色。删除用户或角色时，服务端必须在同一事务中删除对应 `(principal_type, principal_id)` 授权行，再删除主体，避免孤儿授权。

删除最后一个授权主体后保留规则，语义自然收敛为“仅管理员可见”。客户端删除确认展示“将影响 N 条可见范围规则”，不自动把内容恢复为全家可见。

### 4.4 兼容性

现有扩展名称 `accessControl` 和版本保持不变；新增响应字段是向后兼容变化。标准 `/rest/getUsers`、浏览、搜索与媒体端点的响应形状不变。服务端现有浏览、搜索、歌单展开、封面、播放与下载门控测试继续作为不可绕过授权的证据。

## 5. core 与 UniFFI

新增 `core/src/api/access.rs`，只负责请求编排：

```rust
list_access_rules(http, auth) -> Result<Vec<AccessRule>>
set_access_rule(http, auth, scope_type, scope_id, grants) -> Result<AccessRule>
delete_access_rule(http, auth, id) -> Result<()>
```

`set_access_rule` 把每个主体编码为重复查询参数：

```text
grant=user:us-2&grant=role:ro-7
```

空 `grants` 合法，表示仅管理员可见。所有值由 URL 编码器处理，不手写拼接。

`MusicClient` 暴露对应三个 async 方法。`core/src/ffi_types.rs` 为四个 contract 类型声明 UniFFI remote enum/record，Swift 继续直接消费共享 DTO，不复制权限模型。

目标搜索复用现有 OpenSubsonic `search3` 与 `getGenres`；为了 Swift 上下文构造与测试完整，`MusicClientProviding` 同步桥接既有 `getSong`、`getArtist` 和 `listArtists` 能力，但不新增服务端搜索接口。

## 6. Swift 状态边界

新增独立 `AccessControlViewModel`，不把规则状态塞进已经负责用户/角色 CRUD 的 `AdminViewModel`。它持有：

- `rules`、`users`、`roles`
- `query`、`scopeFilter`、`selectedRuleID`
- `targetQuery`、`targetResults`
- `isLoading`、`isSearching`、`isMutating`、`errorMessage`

初始化加载并发读取规则、用户与角色；任一失败时不展示不完整编辑态，并提供重试。写操作串行执行，成功后重新加载三类数据并保留仍存在的规则选择。和 M2A 一样，必须区分“写失败”与“写成功但刷新失败”，后者保留旧状态并提供重新加载。

Swift 视图专用目标类型：

```swift
struct AccessScopeTarget: Identifiable, Hashable {
    let scopeType: ScopeType
    let id: String
    let name: String
    let context: String?
}
```

它不是跨端 DTO，只负责把搜索结果或当前曲库对象转换为编辑器标题。现有规则优先使用 `scopeName`；为空时用不透明 id 和“对象已不存在”兜底。

可选主体规则：普通用户可选；内建 `member` 与自定义角色可选；管理员用户及 `admin` 角色显示为“始终可见”说明，不生成 grant。

## 7. 原生界面

### 7.1 集中访问控制页

在管理员侧栏增加“访问控制”。页面沿用 M2A 的 `HSplitView`：

- 左栏：搜索、作用域筛选、按“曲目 / 专辑 / 艺人 / 流派”分组的规则列表、创建按钮。
- 规则行：目标名、作用域徽标、授权主体数量；空允许名单明确显示“仅管理员”。
- 右栏：目标摘要、当前允许成员与角色、保存按钮、“恢复全家可见”危险操作。
- 空状态：解释默认全家可见，并提供“添加限制”按钮。

新增限制流程先选择作用域，再使用统一搜索：曲目、专辑、艺人来自 `search3`，流派来自 `getGenres` 本地筛选。已存在规则的目标标记“已限制”，选择后直接编辑而不是创建重复规则。

### 7.2 共享规则编辑器

`AccessRuleEditorView` 同时服务集中页和上下文 sheet：

- 顶部显示对象名称、作用域与优先级。
- “家庭成员”与“角色”分区使用 Toggle。
- 明确说明同一用户通过任一直接授权或角色授权即可见。
- 保存时提交完整允许名单，服务端原子替换旧 grants。
- 空名单保存前确认“除管理员外所有成员都将看不到此内容”。
- 已有规则可选择“恢复全家可见”，二次确认后删除规则。

### 7.3 曲库上下文入口

只有管理员登录时显示：

- 专辑网格/列表右键：“设置专辑可见范围”。
- 专辑详情标题菜单：“专辑可见范围”；有 `artistId` 时同时提供“艺人可见范围”。
- 曲目右键：“设置曲目可见范围”。
- 浏览工具栏选择具体流派后：“设置该流派可见范围”。

`LibraryView` 只持有当前待编辑 `AccessScopeTarget?` 并呈现共享 sheet；子视图通过回调上报目标，不直接依赖管理员 ViewModel。普通用户既看不到管理侧栏，也没有任何上下文编辑入口。

## 8. 删除影响与跨页一致性

用户和角色详情页接收只读规则引用计数：删除确认显示会从多少条规则中移除该主体。删除成功后访问控制页下次加载得到清理后的 grants；若某规则因此变为空，集中页显示“仅管理员可见”。

本里程碑不做跨窗口实时同步。所有管理员页在进入时加载，写后刷新；错误条始终提供“重新加载”。

## 9. 错误、并发与可访问性

- 初始加载失败显示具体错误与重试，不把网络失败伪装为空规则集。
- 搜索失败保留已有规则和输入词，只在搜索区域显示错误。
- 同一 ViewModel 的写操作串行化，重复点击不产生乱序覆盖。
- 规则被其他管理员删除后，刷新清除失效选择。
- 所有图标按钮有文字或 help；危险操作使用系统 destructive role；键盘可遍历搜索、列表、Toggle 与保存按钮。
- 颜色只作为辅助信号，作用域和可见状态始终有文字标签。

## 10. TDD 与验证

### 10.1 contract / server

- `AccessRule.scopeName` JSON 往返测试。
- 四种作用域的规则响应都返回正确展示名。
- 已删除目标返回空展示名但规则仍可删除。
- 删除用户/角色清理对应 grants；删除最后授权主体后规则保留且为空名单。
- 现有普通用户管理 API 错误码 50 和全链路访问强制测试继续通过。

### 10.2 core

- 规则列表信封解码四种 scope 与 grants。
- set 请求覆盖重复 grant、空 grant、Unicode 流派名和不透明 id 编码。
- delete 使用 `ru-*` id。
- 服务端失败信封映射为 `CoreError::Server`。

### 10.3 Swift

- 并发加载与选择恢复。
- 作用域/名称筛选和目标搜索转换。
- 管理员主体排除、直接用户与角色授权切换。
- 保存/恢复全家可见成功后刷新。
- 空允许名单确认状态。
- 写失败保留编辑值；写成功但刷新失败不重复提交并可重试。
- 用户/角色删除影响计数。

### 10.4 完成门槛

- `cargo test`、`cargo clippy -- -D warnings`、`cargo fmt --check` 分别对 `contract`、`server`、`core` 全绿。
- `swift build --package-path clients/apple` 与 `swift test --package-path clients/apple` 全绿。
- 启动脚本测试通过。
- 真实服务冒烟覆盖四种 scope：创建规则、更新允许名单、普通用户可见性变化、空名单仅管理员、删除规则恢复默认开放。
- 管理员与普通用户登录分别验证入口显隐。

## 11. 非目标与后续

本里程碑不实现 deny 名单、批量给多个目标套用规则、逐曲有效权限模拟器、角色重命名、规则历史/审计日志、跨窗口实时同步、iOS 界面或离线权限缓存。

M2B 完成后进入现代播放壳层：全局播放队列、底部正在播放栏、进度和音量、系统媒体键、迷你播放器；随后继续艺人导航、收藏/最近播放、现代搜索与大曲库分页。
