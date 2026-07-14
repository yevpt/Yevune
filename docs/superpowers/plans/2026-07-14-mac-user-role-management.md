# Mac 用户与角色管理 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 Yevune macOS 主窗口内交付管理员专用的用户与角色管理工作台，支持账号 CRUD、密码重置、管理员切换、自定义角色 CRUD 与角色分配。

**Architecture:** 服务端新增 `/rest/ext/getUsers` 补齐标准 `getUsers` 缺失的不透明 id 与自定义角色，同时保持 OpenSubsonic 标准响应不变。Rust `core` 统一处理登录权限识别及用户/角色 HTTP 编排，经 UniFFI 暴露共享 `contract` DTO；Swift `AdminViewModel` 负责状态、刷新与误操作护栏，SwiftUI 只渲染和转发操作。

**Tech Stack:** Rust 2021、axum、sqlx/SQLite、reqwest、serde、UniFFI 0.31、Swift 5.9、SwiftUI、XCTest、macOS 14+

## Global Constraints

- 遵守仓库根 `AGENTS.md`；不改变 Rust 服务端、SQLite、Garage、OpenSubsonic 兼容层和原生 UI 红线。
- 标准 `/rest/getUsers` 字段与行为保持不变；新增能力仅放在 `/rest/ext/*` 并通过 `getOpenSubsonicExtensions` 声明。
- `User`/`Role` 只定义在 `contract`，core 通过 UniFFI remote record 暴露，Swift 不复制 DTO。
- 密码不得写日志、持久化、错误文本或测试请求快照的断言输出。
- 每个产品代码任务遵循失败测试 → 确认红灯 → 最小实现 → 确认绿灯 → 提交。
- 不新增依赖；不实现访问规则、播放壳层、离线下载、iOS target 或全局视觉重构。

---

## File Map

| File | Responsibility |
|---|---|
| `server/src/api/ext/user.rs` | 管理员读取完整用户 DTO 的扩展端点。 |
| `server/src/api/ext/mod.rs` | 注册用户管理扩展路由。 |
| `server/src/api/system.rs` | 声明 `userManagement` version 1。 |
| `server/src/openapi.rs` | 登记 `/rest/ext/getUsers`。 |
| `openapi.yaml` | 与服务端 OpenAPI 实现同步的生成产物。 |
| `server/tests/ext_test.rs` | 扩展端点鉴权、字段与发现测试。 |
| `core/src/api/admin.rs` | 当前用户权限识别、用户与角色请求编排。 |
| `core/src/api/mod.rs` | 注册 admin 模块。 |
| `core/src/client.rs` | Session 管理员标记与 UniFFI 门面方法。 |
| `core/src/ffi_types.rs` | `User`/`Role` UniFFI remote records。 |
| `core/tests/admin_test.rs` | core 用户/角色解码和请求编码测试。 |
| `core/tests/login_test.rs` | 登录必须读取当前用户管理员标记。 |
| `core/tests/{browse,delete_move,manage,playlist,scan,upload}_test.rs` | 为登录新增的当前用户请求补齐 mock 响应与请求索引。 |
| `clients/apple/Sources/Yevune/Model/LoginViewModel.swift` | SessionValue 与管理协议接口。 |
| `clients/apple/Sources/Yevune/Model/CoreMusicClient.swift` | Swift 到 UniFFI 的管理接口转发。 |
| `clients/apple/Sources/Yevune/Model/AdminViewModel.swift` | 管理数据、筛选、写操作、刷新和护栏。 |
| `clients/apple/Sources/Yevune/Views/AdminUsersView.swift` | 用户列表、创建与用户详情。 |
| `clients/apple/Sources/Yevune/Views/AdminRolesView.swift` | 角色列表、创建与角色详情。 |
| `clients/apple/Sources/Yevune/Views/LibraryView.swift` | 管理员侧栏入口与详情路由。 |
| `clients/apple/Sources/Yevune/App.swift` | 把登录 SessionValue 注入主窗口。 |
| `clients/apple/Tests/YevuneTests/AdminViewModelTests.swift` | Swift 管理状态与护栏测试。 |
| `clients/apple/Tests/YevuneTests/LoginViewModelTests.swift` | Swift 登录管理员状态桥接回归测试。 |

---

### Task 1: 服务端完整用户列表扩展

**Files:**
- Create: `server/src/api/ext/user.rs`
- Modify: `server/src/api/ext/mod.rs`
- Modify: `server/src/api/system.rs`
- Modify: `server/src/openapi.rs`
- Modify: `openapi.yaml`
- Test: `server/tests/ext_test.rs`

**Interfaces:**
- Consumes: `Index::users().list_users()`、`ApiAdmin`、`response::opaque_id`。
- Produces: authenticated GET `/rest/ext/getUsers` → `{"users":{"user":[contract::User]}}`; extension `userManagement` version 1.

- [ ] **Step 1: 写失败的扩展契约测试**

在 `server/tests/ext_test.rs` 增加：

```rust
#[tokio::test]
async fn admin_get_users_extension_returns_ids_and_custom_roles() {
    let fixture = Fixture::new().await;
    let family_role = fixture.index.roles().create_role("family", false).await.unwrap();
    fixture
        .index
        .roles()
        .assign(fixture.member_id, family_role)
        .await
        .unwrap();

    let body = json(fixture.get("admin", "/rest/ext/getUsers").await).await;
    let users = payload(&body, "users")["user"].as_array().unwrap();
    let member = users.iter().find(|user| user["name"] == "member").unwrap();
    assert_eq!(member["id"], format!("us-{}", fixture.member_id));
    assert_eq!(member["admin"], false);
    assert!(member["roles"].as_array().unwrap().contains(&serde_json::json!("member")));
    assert!(member["roles"].as_array().unwrap().contains(&serde_json::json!("family")));
}

#[tokio::test]
async fn member_cannot_list_users_through_extension() {
    let fixture = Fixture::new().await;
    let body = json(fixture.get("member", "/rest/ext/getUsers").await).await;
    assert_eq!(body["subsonic-response"]["error"]["code"], 50);
}
```

在既有 `extensions_discovery_declares_every_ext_capability` 的名称数组加入 `"userManagement"`。

- [ ] **Step 2: 运行测试确认红灯**

Run: `cargo test --manifest-path server/Cargo.toml --test ext_test -- --nocapture`

Expected: FAIL；`/rest/ext/getUsers` 尚未注册，且扩展发现缺少 `userManagement`。

- [ ] **Step 3: 实现扩展端点与声明**

创建 `server/src/api/ext/user.rs`：

```rust
//! 管理员读取原生客户端所需的完整用户身份与角色。

use axum::extract::{OriginalUri, State};
use axum::response::Response;
use axum::routing::get;
use axum::Router;

use super::super::response::{self, Format};
use super::super::{ApiAdmin, AppState};

pub(super) fn router() -> Router<AppState> {
    Router::new().route("/rest/ext/getUsers", get(get_users))
}

async fn get_users(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    _admin: ApiAdmin,
) -> Response {
    let format = Format::from_uri(&uri);
    match state.index.users().list_users().await {
        Ok(users) => {
            let users = users
                .into_iter()
                .map(|mut user| {
                    user.id = response::opaque_id("user", &user.id);
                    user
                })
                .collect::<Vec<_>>();
            response::ok(format, serde_json::json!({"users": {"user": users}}))
        }
        Err(error) => {
            tracing::error!(%error, "列举完整用户信息失败");
            response::internal(format)
        }
    }
}
```

在 `server/src/api/ext/mod.rs` 声明 `mod user;` 并在 router 中 `.merge(user::router())`。在 `server/src/api/system.rs` 的扩展数组加入：

```rust
{"name": "userManagement", "versions": [1]},
```

在 `server/src/openapi.rs` 的 `ENDPOINTS` 加入：

```rust
("/rest/ext/getUsers", "完整用户列表（扩展，仅管理员）"),
```

从 `server/` 目录运行 `cargo run --bin gen_openapi`，重建根目录 `openapi.yaml`。

- [ ] **Step 4: 确认扩展测试与标准兼容测试为绿**

Run: `cargo test --manifest-path server/Cargo.toml --test ext_test -- --nocapture`

Expected: PASS。

Run: `cargo test --manifest-path server/Cargo.toml --test opensubsonic_test member_can_read_and_change_only_self -- --nocapture`

Expected: PASS；标准用户端点未改变。

- [ ] **Step 5: 提交**

```bash
git add server/src/api/ext/user.rs server/src/api/ext/mod.rs server/src/api/system.rs server/src/openapi.rs server/tests/ext_test.rs openapi.yaml
git commit -m "feat(api): 暴露完整用户管理扩展"
```

---

### Task 2: core 登录权限与只读管理数据

**Files:**
- Create: `core/src/api/admin.rs`
- Create: `core/tests/admin_test.rs`
- Modify: `core/src/api/mod.rs`
- Modify: `core/src/client.rs`
- Modify: `core/src/ffi_types.rs`
- Modify: `core/tests/login_test.rs`
- Modify: `core/tests/browse_test.rs`
- Modify: `core/tests/delete_move_test.rs`
- Modify: `core/tests/manage_test.rs`
- Modify: `core/tests/playlist_test.rs`
- Modify: `core/tests/scan_test.rs`
- Modify: `core/tests/upload_test.rs`

**Interfaces:**
- Consumes: `/rest/getUser?username=`、`/rest/ext/getUsers`、`/rest/ext/getRoles`。
- Produces: `Session { server, user, admin }`, `MusicClient::list_users() -> Result<Vec<User>>`, `MusicClient::list_roles() -> Result<Vec<Role>>`.

- [ ] **Step 1: 改写登录测试并写管理解码失败测试**

把 `core/tests/login_test.rs` 的 mock 改为顺序返回 ping 与当前用户：

```rust
let responses = [
    "{\"subsonic-response\":{\"status\":\"ok\",\"version\":\"1.16.1\"}}",
    "{\"subsonic-response\":{\"status\":\"ok\",\"version\":\"1.16.1\",\"user\":{\"username\":\"admin\",\"adminRole\":true}}}",
];
```

并断言：

```rust
let session = client.login(format!("http://{address}"), "admin".into(), "secret".into()).await.unwrap();
assert!(session.admin);
assert!(paths.lock().await.iter().any(|path| path.contains("/rest/getUser?")));
```

创建 `core/tests/admin_test.rs`，复用 `playlist_test.rs` 的顺序 mock server 形状，加入：

```rust
#[tokio::test]
async fn list_users_and_roles_decode_shared_contract_records() {
    let users = "\"users\":{\"user\":[{\"id\":\"us-1\",\"name\":\"admin\",\"email\":\"a@example.com\",\"created\":null,\"admin\":true,\"roles\":[\"admin\"]}]}";
    let roles = "\"roles\":{\"role\":[{\"id\":\"ro-1\",\"name\":\"admin\",\"isBuiltin\":true}]}";
    let (address, requests, handle) = mock_server(vec![ok(""), current_user(true), ok(users), ok(roles)]).await;
    let client = logged_in(address).await;

    let decoded_users = client.list_users().await.unwrap();
    let decoded_roles = client.list_roles().await.unwrap();
    handle.await.unwrap();

    assert_eq!(decoded_users[0].id, "us-1");
    assert_eq!(decoded_users[0].roles, vec!["admin"]);
    assert_eq!(decoded_roles[0].id, "ro-1");
    assert!(decoded_roles[0].is_builtin);
    let requests = requests.lock().await;
    assert!(requests[2].contains("/rest/ext/getUsers?"));
    assert!(requests[3].contains("/rest/ext/getRoles?"));
}
```

- [ ] **Step 2: 运行测试确认红灯**

Run: `cargo test --manifest-path core/Cargo.toml --test login_test --test admin_test -- --nocapture`

Expected: FAIL；`Session.admin`、admin 模块、`list_users` 与 `list_roles` 尚不存在。

- [ ] **Step 3: 实现只读 admin 模块与 Session 管理员标记**

创建 `core/src/api/admin.rs`：

```rust
//! 管理员用户与角色 API 编排。

use contract::{Role, User};
use serde::Deserialize;

use crate::auth::AuthenticatedSession;
use crate::error::Result;
use crate::http::HttpClient;

pub(crate) async fn current_user_is_admin(
    http: &HttpClient,
    auth: &AuthenticatedSession,
) -> Result<bool> {
    let payload: CurrentUserPayload = http
        .get_json(auth, "getUser", &[("username".into(), auth.user.clone())])
        .await?;
    Ok(payload.user.admin_role)
}

pub(crate) async fn list_users(http: &HttpClient, auth: &AuthenticatedSession) -> Result<Vec<User>> {
    let payload: UsersPayload = http.get_json(auth, "ext/getUsers", &[]).await?;
    Ok(payload.users.user)
}

pub(crate) async fn list_roles(http: &HttpClient, auth: &AuthenticatedSession) -> Result<Vec<Role>> {
    let payload: RolesPayload = http.get_json(auth, "ext/getRoles", &[]).await?;
    Ok(payload.roles.role)
}

#[derive(Deserialize)]
struct CurrentUserPayload { user: CurrentUser }

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurrentUser { admin_role: bool }

#[derive(Deserialize)]
struct UsersPayload { users: UsersBody }

#[derive(Deserialize)]
struct UsersBody { #[serde(default)] user: Vec<User> }

#[derive(Deserialize)]
struct RolesPayload { roles: RolesBody }

#[derive(Deserialize)]
struct RolesBody { #[serde(default)] role: Vec<Role> }
```

在 `core/src/api/mod.rs` 加 `pub(crate) mod admin;`。在 `Session` 加 `pub admin: bool`，并把 login 改为：

```rust
self.http.get_empty(&candidate, "ping").await?;
let admin = admin::current_user_is_admin(&self.http, &candidate).await?;
let session = Session {
    server: candidate.config.public_url(),
    user: candidate.user.clone(),
    admin,
};
```

在 `MusicClient` 导出：

```rust
pub async fn list_users(&self) -> Result<Vec<contract::User>> {
    admin::list_users(&self.http, &self.authenticated_session().await?).await
}

pub async fn list_roles(&self) -> Result<Vec<contract::Role>> {
    admin::list_roles(&self.http, &self.authenticated_session().await?).await
}
```

在 `core/src/ffi_types.rs` 加入 `User`/`Role` import 与 remote record：

```rust
#[uniffi::remote(Record)]
pub struct User {
    pub id: String,
    pub name: String,
    pub email: Option<String>,
    pub created: Option<String>,
    pub admin: bool,
    pub roles: Vec<String>,
}

#[uniffi::remote(Record)]
pub struct Role {
    pub id: String,
    pub name: String,
    pub is_builtin: bool,
}
```

登录现在固定发送 `ping`、`getUser` 两个请求。逐个更新 `core/tests/{browse,delete_move,manage,playlist,scan,upload}_test.rs`：每个成功登录的 mock 响应序列在原 ping 响应之后插入下面的当前用户响应，并把后续请求断言索引统一加一：

```rust
fn current_user(admin: bool) -> String {
    ok(&format!(
        "\"user\":{{\"username\":\"admin\",\"adminRole\":{admin}}}"
    ))
}
```

例如原 `vec![ok(""), ok(tree)]` 改为 `vec![ok(""), current_user(true), ok(tree)]`，原 `requests[1]` 的业务请求断言改为 `requests[2]`。只增加登录响应与索引偏移，不改变各测试原有业务断言。

- [ ] **Step 4: 确认 core 只读测试为绿**

Run: `cargo test --manifest-path core/Cargo.toml --test login_test --test admin_test -- --nocapture`

Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add core/src/api/admin.rs core/src/api/mod.rs core/src/client.rs core/src/ffi_types.rs core/tests/login_test.rs core/tests/admin_test.rs core/tests/browse_test.rs core/tests/delete_move_test.rs core/tests/manage_test.rs core/tests/playlist_test.rs core/tests/scan_test.rs core/tests/upload_test.rs
git commit -m "feat(core): 读取管理员用户与角色"
```

---

### Task 3: core 用户与角色写操作

**Files:**
- Modify: `core/src/api/admin.rs`
- Modify: `core/src/client.rs`
- Test: `core/tests/admin_test.rs`

**Interfaces:**
- Consumes: standard user endpoints and existing `/rest/ext/{createRole,deleteRole,assignRole,unassignRole}`.
- Produces: nine UniFFI async mutation methods with exact names from the design spec.

- [ ] **Step 1: 写所有写操作的请求编码失败测试**

在 `core/tests/admin_test.rs` 加入一个顺序测试，登录响应后依次调用：

```rust
client.create_user("小明".into(), "m@example.com".into(), "s e&c".into(), false).await.unwrap();
client.update_user("小明".into(), "new@example.com".into(), true).await.unwrap();
client.change_password("小明".into(), "new secret".into()).await.unwrap();
let role = client.create_role("孩子".into()).await.unwrap();
client.assign_role("us-2".into(), role.id.clone()).await.unwrap();
client.unassign_role("us-2".into(), role.id.clone()).await.unwrap();
client.delete_role(role.id).await.unwrap();
client.delete_user("小明".into()).await.unwrap();
```

为 mock 的 `createRole` 返回：

```json
{"subsonic-response":{"status":"ok","version":"1.16.1","role":{"id":"ro-9","name":"孩子","isBuiltin":false}}}
```

对记录请求断言 endpoint 与参数包含：`username=%E5%B0%8F%E6%98%8E`、`password=s+e%26c`、`adminRole=false`、`email=new%40example.com`、`userId=us-2`、`roleId=ro-9`。

- [ ] **Step 2: 运行测试确认红灯**

Run: `cargo test --manifest-path core/Cargo.toml --test admin_test write_operations_encode_all_parameters -- --nocapture`

Expected: FAIL；写操作方法尚不存在。

- [ ] **Step 3: 实现 admin 写操作函数**

在 `core/src/api/admin.rs` 增加：

```rust
pub(crate) async fn create_user(http: &HttpClient, auth: &AuthenticatedSession, username: String, email: String, password: String, is_admin: bool) -> Result<()> {
    http.get_empty_with_params(auth, "createUser", &[("username".into(), username), ("email".into(), email), ("password".into(), password), ("adminRole".into(), is_admin.to_string())]).await
}

pub(crate) async fn update_user(http: &HttpClient, auth: &AuthenticatedSession, username: String, email: String, is_admin: bool) -> Result<()> {
    http.get_empty_with_params(auth, "updateUser", &[("username".into(), username), ("email".into(), email), ("adminRole".into(), is_admin.to_string())]).await
}

pub(crate) async fn change_password(http: &HttpClient, auth: &AuthenticatedSession, username: String, password: String) -> Result<()> {
    http.get_empty_with_params(auth, "changePassword", &[("username".into(), username), ("password".into(), password)]).await
}

pub(crate) async fn delete_user(http: &HttpClient, auth: &AuthenticatedSession, username: String) -> Result<()> {
    http.get_empty_with_params(auth, "deleteUser", &[("username".into(), username)]).await
}

pub(crate) async fn create_role(http: &HttpClient, auth: &AuthenticatedSession, name: String) -> Result<Role> {
    let payload: RolePayload = http.get_json(auth, "ext/createRole", &[("name".into(), name)]).await?;
    Ok(payload.role)
}

pub(crate) async fn delete_role(http: &HttpClient, auth: &AuthenticatedSession, id: String) -> Result<()> {
    http.get_empty_with_params(auth, "ext/deleteRole", &[("id".into(), id)]).await
}

pub(crate) async fn assign_role(http: &HttpClient, auth: &AuthenticatedSession, user_id: String, role_id: String) -> Result<()> {
    http.get_empty_with_params(auth, "ext/assignRole", &[("userId".into(), user_id), ("roleId".into(), role_id)]).await
}

pub(crate) async fn unassign_role(http: &HttpClient, auth: &AuthenticatedSession, user_id: String, role_id: String) -> Result<()> {
    http.get_empty_with_params(auth, "ext/unassignRole", &[("userId".into(), user_id), ("roleId".into(), role_id)]).await
}

#[derive(Deserialize)]
struct RolePayload { role: Role }
```

在 `MusicClient` 添加同签名 public async 转发方法，每个方法先取 `authenticated_session()`，再调用相应 `admin::*` 函数。

- [ ] **Step 4: 确认写操作测试与 core 全量为绿**

Run: `cargo test --manifest-path core/Cargo.toml --test admin_test -- --nocapture`

Expected: PASS。

Run: `cargo test --manifest-path core/Cargo.toml`

Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add core/src/api/admin.rs core/src/client.rs core/tests/admin_test.rs
git commit -m "feat(core): 支持用户与角色管理操作"
```

---

### Task 4: 重建 UniFFI 并桥接 Swift 协议

**Files:**
- Modify generated files under: `clients/apple/Packages/YevuneCoreFFI`
- Modify: `clients/apple/Sources/Yevune/Model/LoginViewModel.swift`
- Modify: `clients/apple/Sources/Yevune/Model/CoreMusicClient.swift`
- Modify: `clients/apple/Tests/YevuneTests/LoginViewModelTests.swift`

**Interfaces:**
- Consumes: Task 2/3 UniFFI surface.
- Produces: `MusicClientProviding` admin methods and `SessionValue.admin` with a source-compatible default initializer.

- [ ] **Step 1: 先写 Swift 登录管理员状态失败测试**

在 `LoginViewModelTests.swift` 把 fake login 返回改为 admin，并断言：

```swift
XCTAssertEqual(
    model.session,
    SessionValue(server: "http://music.local:4533", user: "admin", admin: true)
)
```

Fake 增加 `let loginIsAdmin: Bool`，login 返回：

```swift
SessionValue(server: server, user: user, admin: loginIsAdmin)
```

- [ ] **Step 2: 运行 Swift 测试确认红灯**

Run: `swift test --package-path clients/apple --filter LoginViewModelTests/testSubmitPublishesAuthenticatedSession`

Expected: FAIL；`SessionValue` 尚无 `admin`。

- [ ] **Step 3: 重建绑定并扩展 Swift bridge**

Run: `clients/apple/Packages/YevuneCoreFFI/scripts/build-core.sh`

Expected: 生成的 Swift API 包含 `User`、`Role`、`Session.admin` 和管理方法。

把 `SessionValue` 改为：

```swift
struct SessionValue: Equatable {
    let server: String
    let user: String
    let admin: Bool

    init(server: String, user: String, admin: Bool = false) {
        self.server = server
        self.user = user
        self.admin = admin
    }
}
```

在 `MusicClientProviding` 增加 `listUsers/createUser/updateUser/changePassword/deleteUser/listRoles/createRole/deleteRole/assignRole/unassignRole` 声明；在 extension 为这些方法提供 `throw CocoaError(.featureUnsupported)` 默认实现，保持既有 fake 可编译。

在 `CoreMusicClient.login` 返回 `admin: session.admin`，并增加一对一转发，例如：

```swift
func listUsers() async throws -> [User] { try await client.listUsers() }
func createUser(username: String, email: String, password: String, admin: Bool) async throws {
    try await client.createUser(username: username, email: email, password: password, admin: admin)
}
func updateUser(username: String, email: String, admin: Bool) async throws {
    try await client.updateUser(username: username, email: email, admin: admin)
}
func changePassword(username: String, password: String) async throws { try await client.changePassword(username: username, password: password) }
func deleteUser(username: String) async throws { try await client.deleteUser(username: username) }
func listRoles() async throws -> [Role] { try await client.listRoles() }
func createRole(name: String) async throws -> Role { try await client.createRole(name: name) }
func deleteRole(id: String) async throws { try await client.deleteRole(id: id) }
func assignRole(userID: String, roleID: String) async throws { try await client.assignRole(userId: userID, roleId: roleID) }
func unassignRole(userID: String, roleID: String) async throws { try await client.unassignRole(userId: userID, roleId: roleID) }
```

- [ ] **Step 4: 确认 Swift 登录与全量编译为绿**

Run: `swift test --package-path clients/apple --filter LoginViewModelTests`

Expected: PASS。

Run: `swift build --package-path clients/apple`

Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add clients/apple/Packages/YevuneCoreFFI clients/apple/Sources/Yevune/Model/LoginViewModel.swift clients/apple/Sources/Yevune/Model/CoreMusicClient.swift clients/apple/Tests/YevuneTests/LoginViewModelTests.swift
git commit -m "feat(mac): 桥接用户与角色管理接口"
```

---

### Task 5: AdminViewModel 读取、筛选与安全护栏

**Files:**
- Create: `clients/apple/Sources/Yevune/Model/AdminViewModel.swift`
- Create: `clients/apple/Tests/YevuneTests/AdminViewModelTests.swift`

**Interfaces:**
- Consumes: Task 4 `MusicClientProviding` methods and shared `User`/`Role`.
- Produces: observable admin state and pure guard methods consumed by views and Task 6.

- [ ] **Step 1: 写读取、筛选和护栏失败测试**

测试 fixture：两个用户（当前 admin 与 member）和三个角色（admin/member/family）。覆盖：

```swift
func testLoadPublishesUsersRolesAndRestoresSelection() async {
    let fake = FakeAdminClient(users: adminUsers, roles: adminRoles)
    let model = AdminViewModel(client: fake, currentUsername: "admin")
    await model.load()
    model.selectedUserID = "us-2"
    await model.load()
    XCTAssertEqual(model.selectedUserID, "us-2")
    XCTAssertEqual(model.users.count, 2)
    XCTAssertEqual(model.roles.count, 3)
}

func testSearchMatchesNameAndEmailCaseInsensitively() async {
    let model = AdminViewModel(client: FakeAdminClient(users: adminUsers, roles: adminRoles), currentUsername: "admin")
    await model.load()
    model.query = "FAMILY"
    XCTAssertEqual(model.filteredUsers.map(\.id), ["us-2"])
}

func testCurrentAndLastAdminCannotBeDeletedOrDemoted() async {
    let model = AdminViewModel(client: FakeAdminClient(users: [adminUsers[0]], roles: adminRoles), currentUsername: "admin")
    await model.load()
    XCTAssertFalse(model.canDelete(adminUsers[0]))
    XCTAssertFalse(model.canSetAdmin(adminUsers[0], to: false))
}

func testBuiltinRoleCannotBeDeletedAndCustomRoleCountsAffectedUsers() async {
    let model = AdminViewModel(client: FakeAdminClient(users: adminUsers, roles: adminRoles), currentUsername: "admin")
    await model.load()
    XCTAssertFalse(model.canDelete(adminRoles[0]))
    XCTAssertEqual(model.affectedUserCount(for: adminRoles[2]), 1)
}
```

- [ ] **Step 2: 运行测试确认红灯**

Run: `swift test --package-path clients/apple --filter AdminViewModelTests`

Expected: FAIL；`AdminViewModel` 尚不存在。

- [ ] **Step 3: 实现读取状态与纯护栏**

创建 `AdminViewModel.swift`，核心接口如下：

```swift
@MainActor
final class AdminViewModel: ObservableObject {
    @Published private(set) var users: [User] = []
    @Published private(set) var roles: [Role] = []
    @Published var query = ""
    @Published var selectedUserID: String?
    @Published var selectedRoleID: String?
    @Published private(set) var isLoading = false
    @Published private(set) var isMutating = false
    @Published private(set) var errorMessage: String?

    let currentUsername: String
    private let client: any MusicClientProviding

    init(client: any MusicClientProviding, currentUsername: String) {
        self.client = client
        self.currentUsername = currentUsername
    }

    var filteredUsers: [User] {
        let needle = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !needle.isEmpty else { return users }
        return users.filter { $0.name.localizedCaseInsensitiveContains(needle) || ($0.email?.localizedCaseInsensitiveContains(needle) ?? false) }
    }

    var customRoles: [Role] { roles.filter { !$0.isBuiltin } }

    func load() async {
        isLoading = true
        errorMessage = nil
        defer { isLoading = false }
        do {
            async let loadedUsers = client.listUsers()
            async let loadedRoles = client.listRoles()
            users = try await loadedUsers
            roles = try await loadedRoles
            if !users.contains(where: { $0.id == selectedUserID }) { selectedUserID = nil }
            if !roles.contains(where: { $0.id == selectedRoleID }) { selectedRoleID = nil }
        } catch {
            users = []
            roles = []
            errorMessage = error.localizedDescription
        }
    }

    func canDelete(_ user: User) -> Bool {
        user.name != currentUsername && (!user.admin || users.filter(\.admin).count > 1)
    }

    func canSetAdmin(_ user: User, to value: Bool) -> Bool {
        value || !user.admin || users.filter(\.admin).count > 1
    }

    func canDelete(_ role: Role) -> Bool { !role.isBuiltin }
    func affectedUserCount(for role: Role) -> Int { users.filter { $0.roles.contains(role.name) }.count }
}
```

- [ ] **Step 4: 确认 ViewModel 读取测试为绿**

Run: `swift test --package-path clients/apple --filter AdminViewModelTests`

Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add clients/apple/Sources/Yevune/Model/AdminViewModel.swift clients/apple/Tests/YevuneTests/AdminViewModelTests.swift
git commit -m "feat(mac): 建立管理员工作台状态模型"
```

---

### Task 6: AdminViewModel 写操作与刷新

**Files:**
- Modify: `clients/apple/Sources/Yevune/Model/AdminViewModel.swift`
- Modify: `clients/apple/Tests/YevuneTests/AdminViewModelTests.swift`

**Interfaces:**
- Consumes: Task 5 state and guards.
- Produces: view-facing mutation methods; every success reloads users and roles, every failure preserves server state and publishes error.

- [ ] **Step 1: 写写操作、刷新与失败保留测试**

用记录调用的 `FakeAdminClient` 覆盖：创建用户、更新邮箱/管理员、改密码、删除用户、创建/删除角色、分配/解除角色。每项断言调用参数和之后出现 `listUsers`、`listRoles`。另加：

```swift
func testFailedMutationPublishesErrorWithoutReloading() async {
    let fake = FakeAdminClient(users: adminUsers, roles: adminRoles, mutationError: CocoaError(.fileWriteNoPermission))
    let model = AdminViewModel(client: fake, currentUsername: "admin")
    await model.load()
    await model.createRole(name: "family")
    XCTAssertNotNil(model.errorMessage)
    XCTAssertEqual(fake.calls.filter { $0 == "listUsers" }.count, 1)
}
```

- [ ] **Step 2: 运行测试确认红灯**

Run: `swift test --package-path clients/apple --filter AdminViewModelTests`

Expected: FAIL；mutation 方法尚不存在。

- [ ] **Step 3: 实现串行 mutation 包装与公开操作**

在 `AdminViewModel` 增加：

```swift
private func mutate(_ operation: () async throws -> Void) async {
    guard !isMutating else { return }
    isMutating = true
    errorMessage = nil
    defer { isMutating = false }
    do {
        try await operation()
        await load()
    } catch {
        errorMessage = error.localizedDescription
    }
}

func createUser(name: String, email: String, password: String, admin: Bool) async {
    await mutate { try await client.createUser(username: name, email: email, password: password, admin: admin) }
}

func updateUser(_ user: User, email: String, admin: Bool) async {
    guard canSetAdmin(user, to: admin) else { return }
    await mutate { try await client.updateUser(username: user.name, email: email, admin: admin) }
}

func changePassword(for user: User, password: String) async {
    await mutate { try await client.changePassword(username: user.name, password: password) }
}

func deleteUser(_ user: User) async {
    guard canDelete(user) else { return }
    await mutate { try await client.deleteUser(username: user.name) }
}

func createRole(name: String) async { await mutate { _ = try await client.createRole(name: name) } }

func deleteRole(_ role: Role) async {
    guard canDelete(role) else { return }
    await mutate { try await client.deleteRole(id: role.id) }
}

func setRole(_ role: Role, assigned: Bool, for user: User) async {
    await mutate {
        if assigned { try await client.assignRole(userID: user.id, roleID: role.id) }
        else { try await client.unassignRole(userID: user.id, roleID: role.id) }
    }
}
```

- [ ] **Step 4: 确认全部 ViewModel 测试为绿**

Run: `swift test --package-path clients/apple --filter AdminViewModelTests`

Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add clients/apple/Sources/Yevune/Model/AdminViewModel.swift clients/apple/Tests/YevuneTests/AdminViewModelTests.swift
git commit -m "feat(mac): 支持管理员账号与角色操作"
```

---

### Task 7: 用户管理原生界面

**Files:**
- Create: `clients/apple/Sources/Yevune/Views/AdminUsersView.swift`
- Modify: `clients/apple/Sources/Yevune/Views/LibraryView.swift`
- Modify: `clients/apple/Sources/Yevune/App.swift`

**Interfaces:**
- Consumes: `AdminViewModel`, `SessionValue.admin`, `SessionValue.user`.
- Produces: admin-only sidebar entry and complete user list/detail/forms.

- [ ] **Step 1: 扩展根导航模型**

把 `SidebarSelection` 增加 `.adminUsers`。`LibraryView` 接收 `session: SessionValue`，创建：

```swift
@StateObject private var admin: AdminViewModel

init(model: LibraryViewModel, session: SessionValue) {
    self.model = model
    self.session = session
    _media = StateObject(wrappedValue: MediaViewModel(client: model.clientForViews))
    _workflow = StateObject(wrappedValue: LibraryWorkflowViewModel(client: model.clientForViews, library: model))
    _playlists = StateObject(wrappedValue: PlaylistViewModel(client: model.clientForViews))
    _admin = StateObject(wrappedValue: AdminViewModel(client: model.clientForViews, currentUsername: session.user))
}
```

侧栏只在 `session.admin` 时渲染：

```swift
Section("管理") {
    Label("用户", systemImage: "person.2").tag(SidebarSelection.adminUsers)
}
```

`App.swift` 改为 `LibraryView(model: library, session: session)`，其中 `session` 来自 `if let session = login.session`。

- [ ] **Step 2: 创建完整用户列表与详情视图**

`AdminUsersView` 使用 `HSplitView`：左侧搜索框、用户 List、创建按钮；右侧按 `selectedUserID` 显示 `AdminUserDetailView` 或空状态。创建 sheet 字段为用户名、邮箱、初始密码、确认密码、管理员开关，只有名称非空、密码非空且两次一致时可提交。

`AdminUserDetailView` 展示不可编辑用户名、邮箱 TextField、管理员 Toggle、自定义角色 Toggle 列表、保存按钮、重置密码按钮与删除按钮。角色 Toggle 调用 `model.setRole`；删除使用 `confirmationDialog`；`model.canDelete` 与 `model.canSetAdmin` 控制危险操作可用性并给出“必须保留至少一个管理员”说明。

关键接线代码：

```swift
.task { if model.users.isEmpty { await model.load() } }

Button("保存") {
    Task { await model.updateUser(user, email: email, admin: isAdmin) }
}
.disabled(model.isMutating || !model.canSetAdmin(user, to: isAdmin))

ForEach(model.customRoles, id: \.id) { role in
    Toggle(role.name, isOn: Binding(
        get: { user.roles.contains(role.name) },
        set: { assigned in Task { await model.setRole(role, assigned: assigned, for: user) } }
    ))
}
```

- [ ] **Step 3: 接入 detail 路由并编译**

在 `detailContent` 增加：

```swift
case .adminUsers:
    AdminUsersView(model: admin)
```

Run: `swift build --package-path clients/apple`

Expected: PASS。

- [ ] **Step 4: 提交用户页与导航**

```bash
git add clients/apple/Sources/Yevune/Views/AdminUsersView.swift clients/apple/Sources/Yevune/Views/LibraryView.swift clients/apple/Sources/Yevune/App.swift
git commit -m "feat(mac): 加入用户管理工作台"
```

---

### Task 8: 角色管理原生界面与完整 UI 验证

**Files:**
- Create: `clients/apple/Sources/Yevune/Views/AdminRolesView.swift`
- Modify: `clients/apple/Sources/Yevune/Views/LibraryView.swift`

**Interfaces:**
- Consumes: Task 7 root routing and Task 6 role state/mutations.
- Produces: compilable complete M2A SwiftUI workbench.

- [ ] **Step 1: 创建角色列表与详情视图**

`AdminRolesView` 使用 `HSplitView`：左侧把 `roles` 分为“内建角色”和“自定义角色”，提供创建 sheet；右侧显示角色详情。详情展示角色类型、成员列表与删除按钮。内建角色显示“系统角色不可删除”；自定义角色删除确认文案包含：

```swift
Text("将从 \(model.affectedUserCount(for: role)) 位用户移除此角色。")
```

创建角色表单去除首尾空白，空名称禁用提交。删除按钮使用：

```swift
Button("删除角色", role: .destructive) { confirmingDelete = true }
    .disabled(model.isMutating || !model.canDelete(role))
```

- [ ] **Step 2: 编译并运行 Swift 全量测试**

把 `SidebarSelection` 增加 `.adminRoles`，管理员 Section 加入：

```swift
Label("角色", systemImage: "person.badge.key").tag(SidebarSelection.adminRoles)
```

`detailContent` 增加：

```swift
case .adminRoles:
    AdminRolesView(model: admin)
```

Run: `swift build --package-path clients/apple`

Expected: PASS。

Run: `swift test --package-path clients/apple`

Expected: PASS。

- [ ] **Step 3: 提交完整管理员 UI**

```bash
git add clients/apple/Sources/Yevune/Views/AdminRolesView.swift clients/apple/Sources/Yevune/Views/LibraryView.swift
git commit -m "feat(mac): 加入角色管理工作台"
```

---

### Task 9: 全量验证与真实服务冒烟

**Files:**
- Modify only if verification exposes an M2A defect; keep fixes scoped to files listed above.

**Interfaces:**
- Consumes: complete M2A implementation.
- Produces: evidence for every design completion gate.

- [ ] **Step 1: Rust 全量验证**

Run: `cargo test --manifest-path contract/Cargo.toml && cargo test --manifest-path server/Cargo.toml && cargo test --manifest-path core/Cargo.toml`

Expected: PASS。

Run: `cargo clippy --manifest-path contract/Cargo.toml -- -D warnings && cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings && cargo clippy --manifest-path core/Cargo.toml --all-targets -- -D warnings`

Expected: PASS with zero warnings。

Run: `cargo fmt --manifest-path contract/Cargo.toml --check && cargo fmt --manifest-path server/Cargo.toml --check && cargo fmt --manifest-path core/Cargo.toml --check`

Expected: PASS。

- [ ] **Step 2: Swift 与启动脚本验证**

Run: `swift build --package-path clients/apple`

Expected: PASS。

Run: `swift test --package-path clients/apple`

Expected: PASS。

Run: `./scripts/tests/run-mac-client-test.sh`

Expected: PASS。

- [ ] **Step 3: 真实服务手动冒烟**

Run: `./scripts/run-mac-client.sh --with-server`

依次验证并记录结果：管理员登录后能看到管理分区；创建普通用户；创建自定义角色；分配角色；修改邮箱；重置密码；解除并删除角色；删除测试用户；普通用户登录后管理分区消失，直接请求 `/rest/ext/getUsers` 得到错误码 50。

- [ ] **Step 4: 检查提交和工作区**

Run: `git status --short --branch && test -z "$(git rev-list --merges HEAD)"`

Expected: 工作区干净；没有 merge commit；分支仅包含线性小提交。

如果 Step 1–3 发现缺陷，先添加可复现失败测试，再做最小修复，并使用符合范围的 `fix(core): ...`、`fix(api): ...` 或 `fix(mac): ...` 中文提交；修复后从 Step 1 重新执行全量验证。
