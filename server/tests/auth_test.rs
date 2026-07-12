//! auth 层集成测试：密码可逆加密、Subsonic/Bearer 认证、用户/角色管理、提取器。

use md5::{Digest, Md5};
use yevune_server::auth::{verify_subsonic, Encryptor, SubsonicCredentials};
use yevune_server::index::Index;

/// 在临时目录建一个全新索引；返回 TempDir 保活。
async fn temp_index() -> (Index, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("yevune.sqlite");
    let index = Index::connect(&path).await.expect("连接并迁移失败");
    (index, dir)
}

/// 用加密后的密码建一个用户，返回其 id。
async fn seed_user(index: &Index, enc: &Encryptor, name: &str, password: &str) -> i64 {
    index
        .users()
        .create_user(name, &enc.encrypt(password))
        .await
        .unwrap()
}

/// 计算 OpenSubsonic token = hex(md5(密码 + 盐))。
fn subsonic_token(password: &str, salt: &str) -> String {
    let mut h = Md5::new();
    h.update(password.as_bytes());
    h.update(salt.as_bytes());
    hex::encode(h.finalize())
}

// ─────────────────────────── 密码可逆加密 ───────────────────────────

#[test]
fn 加密后可解密还原明文() {
    let enc = Encryptor::new("family-secret");
    let cipher = enc.encrypt("hunter2");
    assert_ne!(cipher, "hunter2", "密文不应等于明文");
    assert_eq!(enc.decrypt(&cipher).unwrap(), "hunter2");
}

#[test]
fn 同一明文两次加密结果不同() {
    let enc = Encryptor::new("family-secret");
    let a = enc.encrypt("hunter2");
    let b = enc.encrypt("hunter2");
    assert_ne!(a, b, "随机 nonce 应使两次密文不同");
    // 但都能解回同一明文
    assert_eq!(enc.decrypt(&a).unwrap(), "hunter2");
    assert_eq!(enc.decrypt(&b).unwrap(), "hunter2");
}

#[test]
fn 错误密钥无法解密() {
    let enc = Encryptor::new("family-secret");
    let cipher = enc.encrypt("hunter2");
    let wrong = Encryptor::new("other-secret");
    assert!(wrong.decrypt(&cipher).is_err(), "错误密钥必须解密失败");
}

#[test]
fn 损坏密文解密失败而非崩溃() {
    let enc = Encryptor::new("family-secret");
    assert!(enc.decrypt("not-base64!!!").is_err());
    assert!(enc.decrypt("").is_err());
}

// ─────────────────────────── Subsonic 认证 ───────────────────────────

#[tokio::test]
async fn subsonic_token_正确被接受() {
    let (index, _dir) = temp_index().await;
    let enc = Encryptor::new("k");
    let id = seed_user(&index, &enc, "alice", "hunter2").await;

    let creds = SubsonicCredentials {
        username: "alice".into(),
        token: Some(subsonic_token("hunter2", "abc")),
        salt: Some("abc".into()),
        password: None,
    };
    let got = verify_subsonic(&index.users(), &enc, &creds).await.unwrap();
    assert_eq!(got, id);
}

#[tokio::test]
async fn subsonic_token_错误被拒() {
    let (index, _dir) = temp_index().await;
    let enc = Encryptor::new("k");
    seed_user(&index, &enc, "alice", "hunter2").await;

    let creds = SubsonicCredentials {
        username: "alice".into(),
        token: Some(subsonic_token("wrong-password", "abc")),
        salt: Some("abc".into()),
        password: None,
    };
    assert!(verify_subsonic(&index.users(), &enc, &creds).await.is_err());
}

#[tokio::test]
async fn subsonic_明文密码正确被接受() {
    let (index, _dir) = temp_index().await;
    let enc = Encryptor::new("k");
    let id = seed_user(&index, &enc, "bob", "s3cret").await;

    let creds = SubsonicCredentials {
        username: "bob".into(),
        token: None,
        salt: None,
        password: Some("s3cret".into()),
    };
    let got = verify_subsonic(&index.users(), &enc, &creds).await.unwrap();
    assert_eq!(got, id);
}

#[tokio::test]
async fn subsonic_明文enc十六进制被接受() {
    let (index, _dir) = temp_index().await;
    let enc = Encryptor::new("k");
    let id = seed_user(&index, &enc, "bob", "s3cret").await;

    let creds = SubsonicCredentials {
        username: "bob".into(),
        token: None,
        salt: None,
        password: Some(format!("enc:{}", hex::encode("s3cret"))),
    };
    let got = verify_subsonic(&index.users(), &enc, &creds).await.unwrap();
    assert_eq!(got, id);
}

#[tokio::test]
async fn subsonic_明文密码错误被拒() {
    let (index, _dir) = temp_index().await;
    let enc = Encryptor::new("k");
    seed_user(&index, &enc, "bob", "s3cret").await;

    let creds = SubsonicCredentials {
        username: "bob".into(),
        token: None,
        salt: None,
        password: Some("nope".into()),
    };
    assert!(verify_subsonic(&index.users(), &enc, &creds).await.is_err());
}

#[tokio::test]
async fn subsonic_未知用户被拒() {
    let (index, _dir) = temp_index().await;
    let enc = Encryptor::new("k");

    let creds = SubsonicCredentials {
        username: "ghost".into(),
        token: Some(subsonic_token("x", "s")),
        salt: Some("s".into()),
        password: None,
    };
    assert!(verify_subsonic(&index.users(), &enc, &creds).await.is_err());
}

#[tokio::test]
async fn subsonic_缺凭证被拒() {
    let (index, _dir) = temp_index().await;
    let enc = Encryptor::new("k");
    seed_user(&index, &enc, "alice", "hunter2").await;

    let creds = SubsonicCredentials {
        username: "alice".into(),
        token: None,
        salt: None,
        password: None,
    };
    assert!(verify_subsonic(&index.users(), &enc, &creds).await.is_err());
}

// ─────────────────────────── Bearer 会话令牌 ───────────────────────────

use std::time::Duration;
use yevune_server::auth::{issue_bearer, issue_bearer_with_expiry, verify_bearer, BearerKey};

#[test]
fn bearer_签发后可校验还原用户id() {
    let key = BearerKey::derive("app-secret");
    let token = issue_bearer(&key, 42, Duration::from_secs(3600));
    assert_eq!(verify_bearer(&key, &token).unwrap(), 42);
}

#[test]
fn bearer_篡改被拒() {
    let key = BearerKey::derive("app-secret");
    let token = issue_bearer(&key, 42, Duration::from_secs(3600));
    let mut bad = token.clone();
    // 改动最后一个字符破坏签名
    bad.pop();
    bad.push(if token.ends_with('A') { 'B' } else { 'A' });
    assert!(verify_bearer(&key, &bad).is_err());
}

#[test]
fn bearer_换密钥被拒() {
    let key = BearerKey::derive("app-secret");
    let token = issue_bearer(&key, 42, Duration::from_secs(3600));
    let other = BearerKey::derive("different-secret");
    assert!(verify_bearer(&other, &token).is_err());
}

#[test]
fn bearer_过期被拒() {
    let key = BearerKey::derive("app-secret");
    // 过期时间设为 1970 之后不久，必已过期
    let token = issue_bearer_with_expiry(&key, 42, 1_000_000);
    assert!(verify_bearer(&key, &token).is_err());
}

#[test]
fn bearer_格式损坏被拒() {
    let key = BearerKey::derive("app-secret");
    assert!(verify_bearer(&key, "garbage").is_err());
    assert!(verify_bearer(&key, "v1.only-two").is_err());
    assert!(verify_bearer(&key, "").is_err());
}

// ─────────────────────────── 用户/角色管理 ───────────────────────────

use yevune_server::auth::UserAdmin;

#[tokio::test]
async fn 创建普通用户赋予member角色且密码可认证() {
    let (index, _dir) = temp_index().await;
    let enc = Encryptor::new("k");
    let admin = UserAdmin::new(&index, &enc);

    let user = admin.create_user("alice", "pw", false).await.unwrap();
    assert_eq!(user.name, "alice");
    assert!(!user.admin);
    assert!(user.roles.contains(&"member".to_string()));

    // 密码经加密存储，可用 Subsonic 明文路径认证
    let creds = SubsonicCredentials {
        username: "alice".into(),
        token: None,
        salt: None,
        password: Some("pw".into()),
    };
    assert!(verify_subsonic(&index.users(), &enc, &creds).await.is_ok());
}

#[tokio::test]
async fn 创建管理员自动建admin角色并判定is_admin() {
    let (index, _dir) = temp_index().await;
    let enc = Encryptor::new("k");
    let admin = UserAdmin::new(&index, &enc);

    let user = admin.create_user("root", "pw", true).await.unwrap();
    assert!(user.admin);
    assert!(user.roles.contains(&"admin".to_string()));
    let id: i64 = user.id.parse().unwrap();
    assert!(admin.is_admin(id).await.unwrap());
}

#[tokio::test]
async fn 改密码后旧密码失效新密码生效() {
    let (index, _dir) = temp_index().await;
    let enc = Encryptor::new("k");
    let admin = UserAdmin::new(&index, &enc);
    let user = admin.create_user("bob", "old", false).await.unwrap();
    let id: i64 = user.id.parse().unwrap();

    assert!(admin.change_password(id, "new").await.unwrap());

    let old = SubsonicCredentials {
        username: "bob".into(),
        token: None,
        salt: None,
        password: Some("old".into()),
    };
    let new = SubsonicCredentials {
        username: "bob".into(),
        token: None,
        salt: None,
        password: Some("new".into()),
    };
    assert!(verify_subsonic(&index.users(), &enc, &old).await.is_err());
    assert!(verify_subsonic(&index.users(), &enc, &new).await.is_ok());
}

#[tokio::test]
async fn 删除用户后不可认证() {
    let (index, _dir) = temp_index().await;
    let enc = Encryptor::new("k");
    let admin = UserAdmin::new(&index, &enc);
    let user = admin.create_user("carol", "pw", false).await.unwrap();
    let id: i64 = user.id.parse().unwrap();

    assert!(admin.delete_user(id).await.unwrap());
    assert!(index.users().get_user(id).await.unwrap().is_none());
}

#[tokio::test]
async fn 重命名用户() {
    let (index, _dir) = temp_index().await;
    let enc = Encryptor::new("k");
    let admin = UserAdmin::new(&index, &enc);
    let user = admin.create_user("dave", "pw", false).await.unwrap();
    let id: i64 = user.id.parse().unwrap();

    assert!(admin.update_user(id, "david").await.unwrap());
    assert_eq!(
        index.users().get_user(id).await.unwrap().unwrap().name,
        "david"
    );
}

#[tokio::test]
async fn 角色创建与删除() {
    let (index, _dir) = temp_index().await;
    let enc = Encryptor::new("k");
    let admin = UserAdmin::new(&index, &enc);

    let role = admin.create_role("kids").await.unwrap();
    assert!(!role.is_builtin);
    let rid: i64 = role.id.parse().unwrap();
    assert!(admin.delete_role(rid).await.unwrap());
}

#[tokio::test]
async fn 内建角色不可删除() {
    let (index, _dir) = temp_index().await;
    let enc = Encryptor::new("k");
    let admin = UserAdmin::new(&index, &enc);
    // 先建管理员以确保内建 admin 角色存在
    admin.create_user("root", "pw", true).await.unwrap();

    let role = index
        .roles()
        .get_role_by_name("admin")
        .await
        .unwrap()
        .unwrap();
    let rid: i64 = role.id.parse().unwrap();
    assert!(
        admin.delete_role(rid).await.is_err(),
        "内建角色必须拒绝删除"
    );
}

#[tokio::test]
async fn 分配与解除角色影响is_admin() {
    let (index, _dir) = temp_index().await;
    let enc = Encryptor::new("k");
    let admin = UserAdmin::new(&index, &enc);

    // 建一个管理员以生成内建 admin 角色，再建一个普通用户
    admin.create_user("root", "pw", true).await.unwrap();
    let user = admin.create_user("eve", "pw", false).await.unwrap();
    let uid: i64 = user.id.parse().unwrap();
    let admin_role = index
        .roles()
        .get_role_by_name("admin")
        .await
        .unwrap()
        .unwrap();
    let arid: i64 = admin_role.id.parse().unwrap();

    assert!(!admin.is_admin(uid).await.unwrap());
    admin.assign_role(uid, arid).await.unwrap();
    assert!(admin.is_admin(uid).await.unwrap());
    assert!(admin.unassign_role(uid, arid).await.unwrap());
    assert!(!admin.is_admin(uid).await.unwrap());
}

// ─────────────────────────── CurrentUser / AdminUser 提取器 ───────────────────────────

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::routing::get;
use axum::Router;
use tower::ServiceExt;
use yevune_server::auth::{AdminUser, AuthState, CurrentUser};

async fn whoami(user: CurrentUser) -> String {
    format!("{}:{}", user.name, user.admin)
}

async fn admin_only(admin: AdminUser) -> String {
    format!("admin:{}", admin.0.name)
}

/// 建一个带 auth 状态的测试路由：/whoami 需任意用户，/admin 需管理员。
fn app(state: AuthState) -> Router {
    Router::new()
        .route("/whoami", get(whoami))
        .route("/admin", get(admin_only))
        .with_state(state)
}

async fn status_of(app: Router, req: Request<Body>) -> StatusCode {
    app.oneshot(req).await.unwrap().status()
}

#[tokio::test]
async fn 提取器_bearer头有效放行() {
    let (index, _dir) = temp_index().await;
    let state = AuthState::new(index.clone(), "app-secret");
    let admin = UserAdmin::new(&index, &state.encryptor);
    let user = admin.create_user("alice", "pw", false).await.unwrap();
    let id: i64 = user.id.parse().unwrap();
    let token = issue_bearer(&state.bearer_key, id, Duration::from_secs(3600));

    let req = Request::builder()
        .uri("/whoami")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(status_of(app(state), req).await, StatusCode::OK);
}

#[tokio::test]
async fn 提取器_bearer无效拒绝401() {
    let (index, _dir) = temp_index().await;
    let state = AuthState::new(index.clone(), "app-secret");

    let req = Request::builder()
        .uri("/whoami")
        .header(header::AUTHORIZATION, "Bearer not-a-valid-token")
        .body(Body::empty())
        .unwrap();
    assert_eq!(status_of(app(state), req).await, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn 提取器_subsonic查询明文放行() {
    let (index, _dir) = temp_index().await;
    let state = AuthState::new(index.clone(), "app-secret");
    let admin = UserAdmin::new(&index, &state.encryptor);
    admin.create_user("bob", "s3cret", false).await.unwrap();

    let req = Request::builder()
        .uri("/whoami?u=bob&p=s3cret")
        .body(Body::empty())
        .unwrap();
    assert_eq!(status_of(app(state), req).await, StatusCode::OK);
}

#[tokio::test]
async fn 提取器_subsonic查询token放行() {
    let (index, _dir) = temp_index().await;
    let state = AuthState::new(index.clone(), "app-secret");
    let admin = UserAdmin::new(&index, &state.encryptor);
    admin.create_user("carol", "pw", false).await.unwrap();
    let token = subsonic_token("pw", "salty");

    let req = Request::builder()
        .uri(format!("/whoami?u=carol&t={token}&s=salty"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(status_of(app(state), req).await, StatusCode::OK);
}

#[tokio::test]
async fn 提取器_无凭证拒绝401() {
    let (index, _dir) = temp_index().await;
    let state = AuthState::new(index.clone(), "app-secret");

    let req = Request::builder()
        .uri("/whoami")
        .body(Body::empty())
        .unwrap();
    assert_eq!(status_of(app(state), req).await, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn 提取器_错误密码拒绝401() {
    let (index, _dir) = temp_index().await;
    let state = AuthState::new(index.clone(), "app-secret");
    let admin = UserAdmin::new(&index, &state.encryptor);
    admin.create_user("bob", "s3cret", false).await.unwrap();

    let req = Request::builder()
        .uri("/whoami?u=bob&p=wrong")
        .body(Body::empty())
        .unwrap();
    assert_eq!(status_of(app(state), req).await, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn 提取器_管理员接口管理员放行() {
    let (index, _dir) = temp_index().await;
    let state = AuthState::new(index.clone(), "app-secret");
    let admin = UserAdmin::new(&index, &state.encryptor);
    admin.create_user("root", "pw", true).await.unwrap();

    let req = Request::builder()
        .uri("/admin?u=root&p=pw")
        .body(Body::empty())
        .unwrap();
    assert_eq!(status_of(app(state), req).await, StatusCode::OK);
}

#[tokio::test]
async fn 提取器_管理员接口普通用户403() {
    let (index, _dir) = temp_index().await;
    let state = AuthState::new(index.clone(), "app-secret");
    let admin = UserAdmin::new(&index, &state.encryptor);
    admin.create_user("member", "pw", false).await.unwrap();

    let req = Request::builder()
        .uri("/admin?u=member&p=pw")
        .body(Body::empty())
        .unwrap();
    assert_eq!(status_of(app(state), req).await, StatusCode::FORBIDDEN);
}
