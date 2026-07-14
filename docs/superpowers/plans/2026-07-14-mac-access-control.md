# Mac 曲库访问控制 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 Yevune macOS 主窗口交付管理员专用的曲库可见范围工作台，支持集中审计规则，并从曲目、专辑、艺人和流派上下文直接编辑用户/角色允许名单。

**Architecture:** `contract::AccessRule` 增加服务端补全的可选展示名，服务端继续强制最具体规则优先并清理被删除主体的授权行。Rust `core` 编排 `/rest/ext/*` 访问规则请求并经 UniFFI 暴露共享 DTO；Swift `AccessControlViewModel` 统一集中页和上下文 sheet 的加载、搜索、写入与错误恢复。

**Tech Stack:** Rust 2021、axum、sqlx/SQLite、reqwest、serde、UniFFI 0.31、Swift 5.9、SwiftUI、XCTest、macOS 14+

## Global Constraints

- 遵守仓库根 `AGENTS.md`；服务端保持 Rust + axum + SQLite/sqlx，客户端保持原生 SwiftUI。
- 默认开放；只有受限对象存规则。允许名单为空表示仅管理员可见。
- 规则优先级固定为曲目 > 专辑 > 艺人 > 流派；管理员始终可见全部。
- 管理能力只使用 `/rest/ext/*`；不改变 OpenSubsonic 标准端点响应形状。
- 跨端 DTO 先改 `contract`，请求逻辑放 `core`，Swift 不复制 `AccessRule`、`Principal` 或枚举。
- 不新增依赖；不实现 deny、批量套用、有效权限模拟、角色重命名、iOS 或离线权限缓存。
- 每个产品代码任务遵循失败测试 → 确认红灯 → 最小实现 → 确认绿灯 → 提交。

---

## File Map

| File | Responsibility |
|---|---|
| `contract/src/access.rs` | 共享访问控制 DTO；增加 `scope_name`。 |
| `contract/tests/serde_test.rs` | `scopeName` 与四种枚举 JSON 往返。 |
| `server/src/index/repo_access.rs` | 仓储构造新增字段，权限判定语义不变。 |
| `server/src/api/ext/access.rs` | 补全规则目标展示名并输出扩展响应。 |
| `server/src/auth/user_admin.rs` | 删除用户/角色时事务清理多态 grants。 |
| `server/src/api/ext/role.rs` | 角色删除统一调用 `UserAdmin`。 |
| `server/tests/ext_test.rs` | 四种 scope 展示名、主体删除清理与管理员鉴权。 |
| `core/src/api/access.rs` | 访问规则 list/set/delete 请求编排。 |
| `core/src/api/mod.rs` | 注册 access 模块。 |
| `core/src/client.rs` | UniFFI 门面访问控制方法。 |
| `core/src/ffi_types.rs` | AccessRule/Principal/ScopeType/PrincipalType remote 类型。 |
| `core/tests/access_test.rs` | 信封解码、重复 grant、空 grant、Unicode 与失败映射。 |
| `clients/apple/Sources/Yevune/Model/LoginViewModel.swift` | 扩展 `MusicClientProviding` 访问控制与目标查询接口。 |
| `clients/apple/Sources/Yevune/Model/CoreMusicClient.swift` | Swift 到 UniFFI 的访问控制转发。 |
| `clients/apple/Sources/Yevune/Model/AccessControlViewModel.swift` | 规则、主体、目标搜索、mutation 与影响计数。 |
| `clients/apple/Sources/Yevune/Views/AdminAccessRulesView.swift` | 集中规则列表、筛选、创建和详情。 |
| `clients/apple/Sources/Yevune/Views/AccessRuleEditorView.swift` | 集中页与上下文共用的允许名单编辑器。 |
| `clients/apple/Sources/Yevune/Views/LibraryView.swift` | 管理侧栏路由、流派入口和共享 sheet。 |
| `clients/apple/Sources/Yevune/Views/AlbumGridView.swift` | 专辑上下文入口。 |
| `clients/apple/Sources/Yevune/Views/MediaDetailView.swift` | 专辑、艺人和曲目上下文入口。 |
| `clients/apple/Sources/Yevune/Views/AdminUsersView.swift` | 用户删除确认展示规则影响数。 |
| `clients/apple/Sources/Yevune/Views/AdminRolesView.swift` | 角色删除确认展示规则影响数。 |
| `clients/apple/Tests/YevuneTests/AccessControlViewModelTests.swift` | Swift 状态、搜索、写入、恢复与影响计数测试。 |

---

### Task 1: 共享规则展示名与服务端响应补全

**Files:**
- Modify: `contract/src/access.rs`
- Modify: `contract/tests/serde_test.rs`
- Modify: `server/src/index/repo_access.rs`
- Modify: `server/src/api/ext/access.rs`
- Modify: `server/tests/ext_test.rs`

**Interfaces:**
- Consumes: 既有 `AccessRule { id, scope_type, scope_id, grants }` 与四种索引表。
- Produces: `AccessRule.scope_name: Option<String>`；`setAccessRule`/`getAccessRules` JSON 字段 `scopeName`。

- [ ] **Step 1: 写 contract 失败测试**

把 `contract/tests/serde_test.rs` 的 AccessRule fixture 改为：

```rust
let rule = AccessRule {
    id: "ru-1".into(),
    scope_type: ScopeType::Album,
    scope_id: "al-2".into(),
    scope_name: Some("Blue Train".into()),
    grants: vec![Principal {
        principal_type: PrincipalType::Role,
        id: "ro-3".into(),
    }],
};
let json = serde_json::to_value(&rule).unwrap();
assert_eq!(json["scopeName"], "Blue Train");
assert_eq!(serde_json::from_value::<AccessRule>(json).unwrap(), rule);
```

- [ ] **Step 2: 运行 contract 测试确认红灯**

Run: `cargo test --manifest-path contract/Cargo.toml --test serde_test access_rule -- --nocapture`

Expected: FAIL；`AccessRule` 尚无 `scope_name`。

- [ ] **Step 3: 增加共享字段并修复仓储构造**

在 `contract/src/access.rs` 的 `AccessRule` 中加入：

```rust
/// 作用域目标展示名；目标已不存在时为空。
pub scope_name: Option<String>,
```

在 `server/src/index/repo_access.rs::hydrate_rule` 构造时加入：

```rust
scope_name: None,
```

同步修正所有测试 fixture，权限仓储本身不解析展示名。

- [ ] **Step 4: 写四种作用域服务端失败测试**

在 `server/tests/ext_test.rs` 新增 `access_rules_include_scope_display_names`：上传一首标题为 `Song A`、专辑为 `Album A`、艺人为 `Artist A`、流派为 `Rock` 的音频，读取其 `track/albumId/artistId`，依次调用：

```rust
for (scope_type, scope_id, expected_name) in [
    ("track", track_id.as_str(), "Song A"),
    ("album", album_id.as_str(), "Album A"),
    ("artist", artist_id.as_str(), "Artist A"),
    ("genre", "Rock", "Rock"),
] {
    let body = json(
        fixture
            .get(
                "admin",
                &format!("/rest/ext/setAccessRule?scopeType={scope_type}&scopeId={scope_id}"),
            )
            .await,
    )
    .await;
    assert_eq!(payload(&body, "accessRule")["scopeName"], expected_name);
}
let listed = json(fixture.get("admin", "/rest/ext/getAccessRules").await).await;
let rules = payload(&listed, "accessRules")["accessRule"].as_array().unwrap();
assert_eq!(rules.len(), 4);
assert!(rules.iter().all(|rule| rule["scopeName"].as_str().is_some()));
```

- [ ] **Step 5: 运行服务端测试确认红灯**

Run: `cargo test --manifest-path server/Cargo.toml --test ext_test access_rules_include_scope_display_names -- --nocapture`

Expected: FAIL；响应尚不含 `scopeName`。

- [ ] **Step 6: 实现异步展示名补全**

在 `server/src/api/ext/access.rs` 加入：

```rust
async fn scope_name(state: &AppState, rule: &contract::AccessRule) -> sqlx::Result<Option<String>> {
    match rule.scope_type {
        ScopeType::Track => sqlx::query_scalar("SELECT title FROM tracks WHERE id = ?")
            .bind(&rule.scope_id).fetch_optional(state.index.pool()).await,
        ScopeType::Album => sqlx::query_scalar("SELECT name FROM albums WHERE id = ?")
            .bind(&rule.scope_id).fetch_optional(state.index.pool()).await,
        ScopeType::Artist => sqlx::query_scalar("SELECT name FROM artists WHERE id = ?")
            .bind(&rule.scope_id).fetch_optional(state.index.pool()).await,
        ScopeType::Genre => Ok(Some(rule.scope_id.clone())),
    }
}

async fn rule_value(
    state: &AppState,
    rule: &contract::AccessRule,
) -> sqlx::Result<serde_json::Value> {
    let scope_name = scope_name(state, rule).await?;
    let scope_id = match rule.scope_type {
        ScopeType::Track => response::opaque_id("track", &rule.scope_id),
        ScopeType::Album => response::opaque_id("album", &rule.scope_id),
        ScopeType::Artist => response::opaque_id("artist", &rule.scope_id),
        ScopeType::Genre => rule.scope_id.clone(),
    };
    Ok(serde_json::json!({
        "id": response::opaque_id("rule", &rule.id),
        "scopeType": rule.scope_type,
        "scopeId": scope_id,
        "scopeName": scope_name,
        "grants": rule.grants.iter().map(|grant| serde_json::json!({
            "type": grant.principal_type,
            "id": response::opaque_id(match grant.principal_type {
                PrincipalType::User => "user",
                PrincipalType::Role => "role",
            }, &grant.id)
        })).collect::<Vec<_>>()
    }))
}
```

`set_rule` 对单条规则 await `rule_value`；`get_rules` 顺序补全到 `Vec<Value>`，任一 SQL 错误返回 `response::internal(format)` 并记录日志。

- [ ] **Step 7: 确认 contract/server 测试为绿并提交**

Run: `cargo test --manifest-path contract/Cargo.toml && cargo test --manifest-path server/Cargo.toml --test ext_test`

Expected: PASS。

```bash
git add contract/src/access.rs contract/tests/serde_test.rs server/src/index/repo_access.rs server/src/api/ext/access.rs server/tests/ext_test.rs
git commit -m "feat(api): 补全访问规则目标名称"
```

---

### Task 2: 删除用户与角色时清理授权主体

**Files:**
- Modify: `server/src/auth/user_admin.rs`
- Modify: `server/src/api/ext/role.rs`
- Modify: `server/tests/ext_test.rs`

**Interfaces:**
- Consumes: `access_rule_grants(rule_id, principal_type, principal_id)`。
- Produces: `UserAdmin::delete_user` / `delete_role` 在同一 SQLite transaction 清理 grants 与主体。

- [ ] **Step 1: 写失败集成测试**

在 `server/tests/ext_test.rs` 新增 `deleting_principals_cleans_access_grants`。测试先创建两个流派规则：一个只授权 member，另一个只授权自定义角色；随后分别调用标准 `deleteUser` 与扩展 `deleteRole`。两者都断言：

```rust
let grants: i64 = sqlx::query_scalar(
    "SELECT COUNT(*) FROM access_rule_grants WHERE principal_type = ? AND principal_id = ?",
)
.bind(principal_type)
.bind(principal_id)
.fetch_one(fixture.index.pool())
.await
.unwrap();
assert_eq!(grants, 0);
let remaining_rules: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM access_rules")
    .fetch_one(fixture.index.pool()).await.unwrap();
assert_eq!(remaining_rules, 1, "规则保留并收敛为仅管理员可见");
```

- [ ] **Step 2: 运行测试确认红灯**

Run: `cargo test --manifest-path server/Cargo.toml --test ext_test deleting_principals_cleans_access_grants -- --nocapture`

Expected: FAIL；主体删除后 grant 仍存在。

- [ ] **Step 3: 实现事务删除**

在 `UserAdmin::delete_user` 中使用 transaction：

```rust
let mut tx = self.index.pool().begin().await?;
sqlx::query("DELETE FROM access_rule_grants WHERE principal_type='user' AND principal_id=?")
    .bind(id).execute(&mut *tx).await?;
let affected = sqlx::query("DELETE FROM users WHERE id=?")
    .bind(id).execute(&mut *tx).await?.rows_affected();
tx.commit().await?;
Ok(affected > 0)
```

在 `UserAdmin::delete_role` 保留内建角色检查，然后执行对应 `'role'` grant 清理与角色删除 transaction。

把 `server/src/api/ext/role.rs::delete_role` 的最终删除改为：

```rust
let admin = crate::auth::UserAdmin::new(&state.index, &state.auth.encryptor);
match admin.delete_role(id).await {
    Ok(true) => response::empty(format),
    Ok(false) => response::not_found(format),
    Err(crate::auth::AuthError::Forbidden) => {
        response::auth_error(format, crate::auth::AuthError::Forbidden)
    }
    Err(error) => {
        tracing::error!(%error, "删除角色失败");
        response::internal(format)
    }
}
```

- [ ] **Step 4: 确认服务端全量测试与 lint 为绿并提交**

Run: `cargo test --manifest-path server/Cargo.toml`

Run: `cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings`

Expected: PASS with zero warnings。

```bash
git add server/src/auth/user_admin.rs server/src/api/ext/role.rs server/tests/ext_test.rs
git commit -m "fix(api): 清理已删除主体的访问授权"
```

---

### Task 3: core 访问控制请求与 UniFFI 类型

**Files:**
- Create: `core/src/api/access.rs`
- Modify: `core/src/api/mod.rs`
- Modify: `core/src/client.rs`
- Modify: `core/src/ffi_types.rs`
- Create: `core/tests/access_test.rs`

**Interfaces:**
- Consumes: `/rest/ext/getAccessRules`、`setAccessRule`、`deleteAccessRule`。
- Produces: `MusicClient::{list_access_rules,set_access_rule,delete_access_rule}` 与 Swift `AccessRule`/`Principal`/枚举。

- [ ] **Step 1: 写 core 失败测试**

创建 `core/tests/access_test.rs`，复用 `admin_test.rs` 的顺序 mock server，覆盖：

```rust
let rules = client.list_access_rules().await.unwrap();
assert_eq!(rules[0].scope_name.as_deref(), Some("摇滚"));
assert_eq!(rules[0].grants[0].id, "us-2");

let saved = client
    .set_access_rule(
        ScopeType::Genre,
        "摇滚 & Blues".into(),
        vec![
            Principal { principal_type: PrincipalType::User, id: "us-2".into() },
            Principal { principal_type: PrincipalType::Role, id: "ro-7".into() },
        ],
    )
    .await
    .unwrap();
assert_eq!(saved.scope_type, ScopeType::Genre);
client
    .set_access_rule(ScopeType::Track, "tr-9".into(), vec![])
    .await
    .unwrap();
client.delete_access_rule("ru-3".into()).await.unwrap();
```

请求快照断言：

```rust
assert!(requests[3].contains("scopeType=genre"));
assert!(requests[3].contains("scopeId=%E6%91%87%E6%BB%9A+%26+Blues"));
assert!(requests[3].contains("grant=user%3Aus-2"));
assert!(requests[3].contains("grant=role%3Aro-7"));
assert!(!requests[4].contains("grant="));
assert!(requests[5].contains("id=ru-3"));
```

另加失败信封断言 `CoreError::Server { code: 50, .. }`。

- [ ] **Step 2: 运行测试确认红灯**

Run: `cargo test --manifest-path core/Cargo.toml --test access_test -- --nocapture`

Expected: FAIL；访问控制方法和 UniFFI remote 类型尚不存在。

- [ ] **Step 3: 声明 remote 类型**

在 `core/src/ffi_types.rs` 引入四个 contract 类型并加入：

```rust
#[uniffi::remote(Enum)]
pub enum ScopeType { Track, Album, Artist, Genre }

#[uniffi::remote(Enum)]
pub enum PrincipalType { User, Role }

#[uniffi::remote(Record)]
pub struct Principal {
    pub principal_type: PrincipalType,
    pub id: String,
}

#[uniffi::remote(Record)]
pub struct AccessRule {
    pub id: String,
    pub scope_type: ScopeType,
    pub scope_id: String,
    pub scope_name: Option<String>,
    pub grants: Vec<Principal>,
}
```

- [ ] **Step 4: 实现 core access 模块**

创建 `core/src/api/access.rs`：

```rust
use contract::{AccessRule, Principal, PrincipalType, ScopeType};
use serde::Deserialize;
use crate::auth::AuthenticatedSession;
use crate::error::Result;
use crate::http::HttpClient;

pub(crate) async fn list_access_rules(
    http: &HttpClient,
    auth: &AuthenticatedSession,
) -> Result<Vec<AccessRule>> {
    let payload: AccessRulesPayload = http.get_json(auth, "ext/getAccessRules", &[]).await?;
    Ok(payload.access_rules.access_rule)
}

pub(crate) async fn set_access_rule(
    http: &HttpClient,
    auth: &AuthenticatedSession,
    scope_type: ScopeType,
    scope_id: String,
    grants: Vec<Principal>,
) -> Result<AccessRule> {
    let scope = match scope_type {
        ScopeType::Track => "track",
        ScopeType::Album => "album",
        ScopeType::Artist => "artist",
        ScopeType::Genre => "genre",
    };
    let mut params = vec![
        ("scopeType".into(), scope.into()),
        ("scopeId".into(), scope_id),
    ];
    params.extend(grants.into_iter().map(|grant| {
        let kind = match grant.principal_type {
            PrincipalType::User => "user",
            PrincipalType::Role => "role",
        };
        ("grant".into(), format!("{kind}:{}", grant.id))
    }));
    let payload: AccessRulePayload = http.get_json(auth, "ext/setAccessRule", &params).await?;
    Ok(payload.access_rule)
}

pub(crate) async fn delete_access_rule(
    http: &HttpClient,
    auth: &AuthenticatedSession,
    id: String,
) -> Result<()> {
    http.get_empty_with_params(auth, "ext/deleteAccessRule", &[("id".into(), id)]).await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccessRulesPayload { access_rules: AccessRulesBody }
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccessRulesBody { #[serde(default)] access_rule: Vec<AccessRule> }
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccessRulePayload { access_rule: AccessRule }
```

在 `api/mod.rs` 注册模块，在 `MusicClient` 添加同签名 public async 转发方法。

- [ ] **Step 5: 确认 core 全量测试与 lint 为绿并提交**

Run: `cargo test --manifest-path core/Cargo.toml`

Run: `cargo clippy --manifest-path core/Cargo.toml --all-targets -- -D warnings`

Expected: PASS with zero warnings。

```bash
git add core/src/api/access.rs core/src/api/mod.rs core/src/client.rs core/src/ffi_types.rs core/tests/access_test.rs
git commit -m "feat(core): 支持曲库访问规则管理"
```

---

### Task 4: 重建绑定并桥接 Swift 协议

**Files:**
- Modify: `clients/apple/Sources/Yevune/Model/LoginViewModel.swift`
- Modify: `clients/apple/Sources/Yevune/Model/CoreMusicClient.swift`
- Test: `clients/apple/Tests/YevuneTests/LoginViewModelTests.swift`

**Interfaces:**
- Consumes: Task 3 UniFFI methods与既有 `getSong/getArtist/listArtists`。
- Produces: `MusicClientProviding` 规则 CRUD 与完整目标查询接口。

- [ ] **Step 1: 重建本地绑定并核对生成名称**

Run: `clients/apple/Packages/YevuneCoreFFI/scripts/build-core.sh`

Run: `rg -n "struct AccessRule|enum ScopeType|listAccessRules|setAccessRule|deleteAccessRule" clients/apple/Packages/YevuneCoreFFI/Sources/YevuneCoreFFI/YevuneCoreFFI.swift`

Expected: 生成 `AccessRule`、`Principal`、`ScopeType`、`PrincipalType` 与三个 async 方法。生成源码和 xcframework 被 gitignore，不提交。

- [ ] **Step 2: 写协议桥接失败编译测试**

在 `LoginViewModelTests` 的 fake 加入访问规则返回，并在 production bridge 测试中声明：

```swift
let client: any MusicClientProviding = CoreMusicClient()
_ = client
```

随后在测试中直接调用 fake 的 `listAccessRules`，使缺少协议方法时编译失败。

- [ ] **Step 3: 扩展协议与默认实现**

在 `MusicClientProviding` 增加：

```swift
func listAccessRules() async throws -> [AccessRule]
func setAccessRule(scopeType: ScopeType, scopeID: String, grants: [Principal]) async throws -> AccessRule
func deleteAccessRule(id: String) async throws
func getSong(id: String) async throws -> Track
func getArtist(id: String) async throws -> ArtistDetail
func listArtists() async throws -> [Artist]
```

extension 默认实现统一抛 `CocoaError(.featureUnsupported)`，避免既有测试 fake 批量修改。

在 `CoreMusicClient` 一对一转发，参数标签严格按生成 Swift API：

```swift
func listAccessRules() async throws -> [AccessRule] { try await client.listAccessRules() }
func setAccessRule(scopeType: ScopeType, scopeID: String, grants: [Principal]) async throws -> AccessRule {
    try await client.setAccessRule(scopeType: scopeType, scopeId: scopeID, grants: grants)
}
func deleteAccessRule(id: String) async throws { try await client.deleteAccessRule(id: id) }
func getSong(id: String) async throws -> Track { try await client.getSong(id: id) }
func getArtist(id: String) async throws -> ArtistDetail { try await client.getArtist(id: id) }
func listArtists() async throws -> [Artist] { try await client.listArtists() }
```

- [ ] **Step 4: 编译、测试并提交**

Run: `swift build --package-path clients/apple`

Run: `swift test --package-path clients/apple`

Expected: PASS。

```bash
git add clients/apple/Sources/Yevune/Model/LoginViewModel.swift clients/apple/Sources/Yevune/Model/CoreMusicClient.swift clients/apple/Tests/YevuneTests/LoginViewModelTests.swift
git commit -m "feat(mac): 桥接曲库访问控制接口"
```

---

### Task 5: AccessControlViewModel 读取、筛选与目标搜索

**Files:**
- Create: `clients/apple/Sources/Yevune/Model/AccessControlViewModel.swift`
- Create: `clients/apple/Tests/YevuneTests/AccessControlViewModelTests.swift`

**Interfaces:**
- Consumes: Task 4 `MusicClientProviding`、共享 DTO。
- Produces: `AccessScopeTarget`、规则/主体状态、筛选和 `searchTargets(scopeType:query:)`。

- [ ] **Step 1: 写读取与搜索失败测试**

创建 actor fake 和测试 fixture，覆盖：

```swift
let model = AccessControlViewModel(client: fake)
model.selectedRuleID = "ru-2"
await model.load()
XCTAssertEqual(model.rules, rules)
XCTAssertEqual(model.selectedRuleID, "ru-2")
XCTAssertEqual(model.assignableUsers.map(\.name), ["listener"])
XCTAssertFalse(model.assignableRoles.contains(where: { $0.name == "admin" }))

model.query = "BLUE"
XCTAssertEqual(model.filteredRules.map(\.id), ["ru-2"])

await model.searchTargets(scopeType: .album, query: "blue")
XCTAssertEqual(model.targetResults.first?.name, "Blue Train")
XCTAssertEqual(model.targetResults.first?.scopeType, .album)

await model.searchTargets(scopeType: .genre, query: "rock")
XCTAssertEqual(model.targetResults.map(\.name), ["Rock"])
```

另测加载任一请求失败时 `rules/users/roles` 全部为空、`errorMessage` 非空、选择清除。

- [ ] **Step 2: 运行测试确认红灯**

Run: `swift test --package-path clients/apple --filter AccessControlViewModelTests`

Expected: FAIL；类型尚不存在。

- [ ] **Step 3: 实现目标类型与读取状态**

创建：

```swift
import Foundation
import YevuneCoreFFI

struct AccessScopeTarget: Identifiable, Hashable {
    let scopeType: ScopeType
    let id: String
    let name: String
    let context: String?
}

@MainActor
final class AccessControlViewModel: ObservableObject {
    @Published private(set) var rules: [AccessRule] = []
    @Published private(set) var users: [User] = []
    @Published private(set) var roles: [Role] = []
    @Published private(set) var targetResults: [AccessScopeTarget] = []
    @Published var query = ""
    @Published var scopeFilter: ScopeType?
    @Published var selectedRuleID: String?
    @Published private(set) var isLoading = false
    @Published private(set) var isSearching = false
    @Published private(set) var isMutating = false
    @Published private(set) var errorMessage: String?
    private let client: any MusicClientProviding

    init(client: any MusicClientProviding) { self.client = client }

    var assignableUsers: [User] { users.filter { !$0.admin } }
    var assignableRoles: [Role] { roles.filter { $0.name != "admin" } }
}
```

`load()` 使用三个 `async let`；成功后整体 apply 并按 id 保留选择，失败时清空全部可编辑状态。

- [ ] **Step 4: 实现筛选与搜索转换**

`filteredRules` 同时应用 `scopeFilter` 和 `scopeName/scopeId` 大小写不敏感搜索。`searchTargets`：

```swift
func searchTargets(scopeType: ScopeType, query: String) async {
    let needle = query.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !needle.isEmpty else { targetResults = []; return }
    isSearching = true
    defer { isSearching = false }
    do {
        if scopeType == .genre {
            targetResults = try await client.listGenres()
                .filter { $0.value.localizedCaseInsensitiveContains(needle) }
                .map { AccessScopeTarget(scopeType: .genre, id: $0.value, name: $0.value, context: "\($0.songCount) 首") }
        } else {
            let result = try await client.search(query: needle)
            switch scopeType {
            case .track:
                targetResults = result.tracks.map { .init(scopeType: .track, id: $0.id, name: $0.title, context: $0.album) }
            case .album:
                targetResults = result.albums.map { .init(scopeType: .album, id: $0.id, name: $0.name, context: $0.artist) }
            case .artist:
                targetResults = result.artists.map { .init(scopeType: .artist, id: $0.id, name: $0.name, context: "\($0.albumCount) 张专辑") }
            case .genre:
                targetResults = []
            }
        }
    } catch {
        errorMessage = error.localizedDescription
    }
}
```

- [ ] **Step 5: 确认测试为绿并提交**

Run: `swift test --package-path clients/apple --filter AccessControlViewModelTests`

Expected: PASS。

```bash
git add clients/apple/Sources/Yevune/Model/AccessControlViewModel.swift clients/apple/Tests/YevuneTests/AccessControlViewModelTests.swift
git commit -m "feat(mac): 建立访问控制状态模型"
```

---

### Task 6: 规则写入、错误恢复与删除影响计数

**Files:**
- Modify: `clients/apple/Sources/Yevune/Model/AccessControlViewModel.swift`
- Modify: `clients/apple/Tests/YevuneTests/AccessControlViewModelTests.swift`

**Interfaces:**
- Consumes: Task 5 状态。
- Produces: `saveRule`、`restoreFamilyVisibility`、`rule(for:)`、主体影响计数与 mutation Bool 结果。

- [ ] **Step 1: 写 mutation 失败测试**

fake 记录精确参数并可分别让写请求或写后第二次 list 失败。覆盖：

```swift
let target = AccessScopeTarget(scopeType: .genre, id: "摇滚 & Blues", name: "摇滚 & Blues", context: nil)
let grants = [
    Principal(principalType: .user, id: "us-2"),
    Principal(principalType: .role, id: "ro-7"),
]
XCTAssertTrue(await model.saveRule(target: target, grants: grants))
XCTAssertTrue(await fake.calls().contains(.set(.genre, "摇滚 & Blues", grants)))
XCTAssertTrue(await model.restoreFamilyVisibility(ruleID: "ru-1"))
XCTAssertTrue(await fake.calls().contains(.delete("ru-1")))
```

另测：写失败返回 false、保留规则；写成功但刷新失败返回 true、保留旧规则并显示“操作已完成”；`requiresEmptyGrantConfirmation([]) == true`；用户/角色引用计数正确。

- [ ] **Step 2: 运行测试确认红灯**

Run: `swift test --package-path clients/apple --filter AccessControlViewModelTests`

Expected: FAIL；写方法不存在。

- [ ] **Step 3: 实现查询与影响计数**

```swift
func rule(for target: AccessScopeTarget) -> AccessRule? {
    rules.first { $0.scopeType == target.scopeType && $0.scopeId == target.id }
}

func ruleReferenceCount(userID: String) -> Int {
    rules.count { $0.grants.contains { $0.principalType == .user && $0.id == userID } }
}

func ruleReferenceCount(roleID: String) -> Int {
    rules.count { $0.grants.contains { $0.principalType == .role && $0.id == roleID } }
}

func requiresEmptyGrantConfirmation(_ grants: [Principal]) -> Bool { grants.isEmpty }
```

- [ ] **Step 4: 实现可区分写/刷新结果的 mutation**

沿用 M2A 已验证模式：`mutate` 先执行 write，write 失败返回 false；write 成功后并发 reload，reload 失败保留旧状态并返回 true。公开方法：

```swift
@discardableResult
func saveRule(target: AccessScopeTarget, grants: [Principal]) async -> Bool {
    await mutate {
        _ = try await client.setAccessRule(scopeType: target.scopeType, scopeID: target.id, grants: grants)
    }
}

@discardableResult
func restoreFamilyVisibility(ruleID: String) async -> Bool {
    await mutate { try await client.deleteAccessRule(id: ruleID) }
}
```

- [ ] **Step 5: 确认 Swift 全量测试为绿并提交**

Run: `swift test --package-path clients/apple`

Expected: PASS。

```bash
git add clients/apple/Sources/Yevune/Model/AccessControlViewModel.swift clients/apple/Tests/YevuneTests/AccessControlViewModelTests.swift
git commit -m "feat(mac): 支持访问规则编辑与恢复"
```

---

### Task 7: 集中访问控制页与共享编辑器

**Files:**
- Create: `clients/apple/Sources/Yevune/Views/AdminAccessRulesView.swift`
- Create: `clients/apple/Sources/Yevune/Views/AccessRuleEditorView.swift`

**Interfaces:**
- Consumes: Task 6 ViewModel。
- Produces: 可编译的集中规则审计、目标搜索与规则编辑视图。

- [ ] **Step 1: 创建共享编辑器**

`AccessRuleEditorView` 接收：

```swift
let target: AccessScopeTarget
@ObservedObject var model: AccessControlViewModel
let onComplete: () -> Void
```

内部 `@State var selectedUserIDs` / `selectedRoleIDs` 从 `model.rule(for:)` 初始化。保存时把排序后的 id 映射成 `Principal`；空数组先显示确认 dialog。已有规则显示“恢复全家可见”按钮，调用成功后 `onComplete()`。管理员说明固定显示，不为 admin 生成 Toggle。

- [ ] **Step 2: 创建集中规则页面**

`AdminAccessRulesView` 使用 `HSplitView`：左栏搜索、scope Picker、按 scope 分组的 List 和“添加限制”；右栏把选中规则转换为：

```swift
AccessScopeTarget(
    scopeType: rule.scopeType,
    id: rule.scopeId,
    name: rule.scopeName ?? "对象已不存在",
    context: rule.scopeName == nil ? rule.scopeId : nil
)
```

“添加限制”sheet 先用 segmented scope Picker，再调用 `model.searchTargets`，选择结果后切换到同一个 `AccessRuleEditorView`。空规则页说明“默认全家可见”。初始失败页与非空错误条都提供 `model.load()` 重试。

- [ ] **Step 3: 编译并提交集中 UI**

Run: `swift build --package-path clients/apple`

Expected: PASS。

```bash
git add clients/apple/Sources/Yevune/Views/AdminAccessRulesView.swift clients/apple/Sources/Yevune/Views/AccessRuleEditorView.swift
git commit -m "feat(mac): 加入访问控制工作台"
```

---

### Task 8: 管理侧栏与四种曲库上下文入口

**Files:**
- Modify: `clients/apple/Sources/Yevune/Views/LibraryView.swift`
- Modify: `clients/apple/Sources/Yevune/Views/AlbumGridView.swift`
- Modify: `clients/apple/Sources/Yevune/Views/MediaDetailView.swift`
- Modify: `clients/apple/Sources/Yevune/Views/AdminUsersView.swift`
- Modify: `clients/apple/Sources/Yevune/Views/AdminRolesView.swift`

**Interfaces:**
- Consumes: Task 7 页面与编辑器、`SessionValue.admin`。
- Produces: 管理员双入口；普通用户完全不渲染入口。

- [ ] **Step 1: 扩展根状态与管理路由**

`SidebarSelection` 加 `.adminAccess`；`LibraryView` 创建：

```swift
@StateObject private var access: AccessControlViewModel
@State private var accessTarget: AccessScopeTarget?
```

init 使用 `model.clientForViews`。管理员 Section 加：

```swift
Label("访问控制", systemImage: "eye.badge")
    .tag(SidebarSelection.adminAccess)
```

detail route 返回 `AdminAccessRulesView(model: access)`；用户/角色页面传入同一 access model。根视图用 `accessTarget != nil` 呈现 `AccessRuleEditorView` sheet。

- [ ] **Step 2: 增加专辑入口**

`AlbumGridView` 增加带默认值的可选回调，使既有测试和调用保持源码兼容：

```swift
let onManageAccess: ((AccessScopeTarget) -> Void)? = nil
```

cell context menu 只在回调非空时显示“设置专辑可见范围”，构造 `.album` target。Library 的列表模式专辑行增加相同 context menu。普通用户传 nil。

- [ ] **Step 3: 增加艺人、专辑与曲目详情入口**

`MediaDetailView` 增加同一可选回调。专辑标题旁 Menu 提供专辑 target；`album.artistId` 非空时提供艺人 target。每个 track context menu 提供 `.track` target。LibraryView 只在 `session.admin` 时传闭包 `{ accessTarget = $0 }`。

- [ ] **Step 4: 增加流派入口**

在 `browseToolbar` 中，当 `session.admin` 且 `model.genreFilter` 非空时显示：

```swift
Button {
    if let genre = model.genreFilter {
        accessTarget = .init(scopeType: .genre, id: genre, name: genre, context: nil)
    }
} label: {
    Label("可见范围", systemImage: "eye")
}
```

- [ ] **Step 5: 接入用户/角色删除影响文案**

`AdminUsersView` 与 `AdminRolesView` 增加 `@ObservedObject var access: AccessControlViewModel`。删除确认 message 分别追加：

```swift
Text("该用户会从 \(access.ruleReferenceCount(userID: user.id)) 条可见范围规则中移除。")
Text("该角色会从 \(access.ruleReferenceCount(roleID: role.id)) 条可见范围规则中移除。")
```

根路由传入同一个 access model；管理员页面 `.task` 在规则尚未加载时调用 `access.load()`，使影响数来自服务端状态。

- [ ] **Step 6: 编译、全量 Swift 测试并提交**

Run: `swift build --package-path clients/apple`

Run: `swift test --package-path clients/apple`

Expected: PASS。

```bash
git add clients/apple/Sources/Yevune/Views/LibraryView.swift clients/apple/Sources/Yevune/Views/AlbumGridView.swift clients/apple/Sources/Yevune/Views/MediaDetailView.swift clients/apple/Sources/Yevune/Views/AdminUsersView.swift clients/apple/Sources/Yevune/Views/AdminRolesView.swift
git commit -m "feat(mac): 接入曲库可见范围快捷入口"
```

---

### Task 9: 全量验证、真实服务冒烟与代码审查

**Files:**
- Modify only files above if verification exposes a scoped M2B defect.

**Interfaces:**
- Consumes: complete M2B implementation.
- Produces: design §10.4 每项完成门槛的证据。

- [ ] **Step 1: Rust 全量验证**

Run: `cargo test --manifest-path contract/Cargo.toml`

Run: `cargo test --manifest-path server/Cargo.toml`

Run: `cargo test --manifest-path core/Cargo.toml`

Run: `cargo clippy --manifest-path contract/Cargo.toml -- -D warnings`

Run: `cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings`

Run: `cargo clippy --manifest-path core/Cargo.toml --all-targets -- -D warnings`

Run: `cargo fmt --manifest-path contract/Cargo.toml --check`

Run: `cargo fmt --manifest-path server/Cargo.toml --check`

Run: `cargo fmt --manifest-path core/Cargo.toml --check`

Expected: all exit 0, zero warnings and zero failures。

- [ ] **Step 2: Swift 与启动脚本验证**

Run: `swift build --package-path clients/apple`

Run: `swift test --package-path clients/apple`

Run: `./scripts/tests/run-mac-client-test.sh`

Expected: all PASS。

- [ ] **Step 3: 隔离真实服务冒烟**

用临时 SQLite 启动 `yevune-server`，创建普通用户与自定义角色，准备一首带专辑/艺人/流派的测试音频。依次对 track/album/artist/genre 调 `setAccessRule`，验证：普通用户无 grant 时对应浏览/搜索/播放不可见；加直接用户或角色 grant 后可见；空 grant 只有管理员可见；`deleteAccessRule` 后恢复默认开放；删除授权用户/角色后数据库无孤儿 grants。

- [ ] **Step 4: 独立代码审查**

审查范围从 `5a5add5` 后首个实现提交到 HEAD，要求检查服务端授权不可绕过、空名单语义、主体删除事务、Swift 普通用户入口隐藏、mutation/refresh 区分与测试覆盖。Critical/Important 必须修复并重新验证。

- [ ] **Step 5: 收束审查结果**

若没有 Critical/Important，记录 Ready to merge。若发现问题，回到对应 Task 的测试文件先补失败用例，完成红→绿后以 `fix(api): 修正访问规则服务端问题`、`fix(core): 修正访问规则客户端问题` 或 `fix(mac): 修正访问控制交互问题` 中与实际层次一致的一条提交，并重新执行 Step 1–4；禁止创建空提交。
