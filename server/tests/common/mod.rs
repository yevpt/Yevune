//! HTTP 集成测试共享脚手架：装配带 [`MemoryStore`] 的应用、签发 Bearer 令牌、播种数据。
#![allow(dead_code)]

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use music_server::auth::{issue_bearer, AuthState};
use music_server::index::{Index, NewTrack};
use music_server::storage::MemoryStore;
use music_server::{app_with_state, AppState};
use tempfile::TempDir;
use tower::ServiceExt;

/// 测试上下文：应用 + 索引 + 内存存储 + 认证句柄（保活临时目录）。
pub struct Ctx {
    pub index: Index,
    pub store: Arc<MemoryStore>,
    app: axum::Router,
    auth: AuthState,
    _dir: TempDir,
}

/// 装配一个全新测试应用（临时 SQLite + 内存对象存储）。
pub async fn ctx() -> Ctx {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("music.sqlite");
    let index = Index::connect(&path).await.expect("连接并迁移失败");
    let store = Arc::new(MemoryStore::new());
    let state = AppState::new(
        index.clone(),
        "test-secret",
        store.clone() as Arc<dyn music_server::storage::ObjectStore>,
    );
    let auth = state.auth.clone();
    let app = app_with_state(state);
    Ctx {
        index,
        store,
        app,
        auth,
        _dir: dir,
    }
}

impl Ctx {
    /// 为用户签发 1 小时有效的 Bearer 令牌。
    pub fn bearer(&self, user_id: i64) -> String {
        issue_bearer(&self.auth.bearer_key, user_id, Duration::from_secs(3600))
    }

    /// 建用户并（可选）赋角色，返回用户 id。
    pub async fn create_user(&self, name: &str, roles: &[&str]) -> i64 {
        let uid = self.index.users().create_user(name, "enc").await.unwrap();
        for r in roles {
            let rid = match self.index.roles().get_role_by_name(r).await.unwrap() {
                Some(role) => role.id.parse().unwrap(),
                None => self.index.roles().create_role(r, false).await.unwrap(),
            };
            self.index.roles().assign(uid, rid).await.unwrap();
        }
        uid
    }

    /// 发起 GET 请求，返回 (状态码, 响应体字符串)。`bearer` 为 `Some` 时带 Authorization 头。
    pub async fn get(&self, uri: &str, bearer: Option<&str>) -> (StatusCode, String) {
        let mut builder = Request::builder().uri(uri);
        if let Some(token) = bearer {
            builder = builder.header("Authorization", format!("Bearer {token}"));
        }
        let response = self
            .app
            .clone()
            .oneshot(builder.body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, String::from_utf8(bytes.to_vec()).unwrap())
    }

    /// 发起 GET 请求并把 JSON 响应体解析为 `serde_json::Value`。
    pub async fn get_json(
        &self,
        uri: &str,
        bearer: Option<&str>,
    ) -> (StatusCode, serde_json::Value) {
        let (status, body) = self.get(uri, bearer).await;
        let json = serde_json::from_str(&body).unwrap_or(serde_json::Value::Null);
        (status, json)
    }
}

/// 构造一条最小 NewTrack。
pub fn track(title: &str, album_id: Option<i64>, artist_id: Option<i64>, key: &str) -> NewTrack {
    NewTrack {
        title: title.into(),
        album_id,
        artist_id,
        track_no: Some(1),
        duration: Some(200),
        codec: Some("flac".into()),
        bitrate: Some(1000),
        size: Some(30_000_000),
        object_key: key.into(),
        ..Default::default()
    }
}
