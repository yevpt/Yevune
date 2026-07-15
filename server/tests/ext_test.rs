//! `/rest/ext/*` 自研扩展接口集成测试。

use std::collections::HashSet;
use std::ops::Range;
use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use bytes::Bytes;
use futures::{stream, StreamExt};
use http_body_util::BodyExt;
use tokio::sync::Notify;
use tower::ServiceExt;
use yevune_server::api::AppState;
use yevune_server::auth::{Encryptor, UserAdmin};
use yevune_server::index::Index;
use yevune_server::storage::{
    ListPage, MemoryStore, ObjectMeta, ObjectStore, Result as StorageResult, StorageError,
};

struct Fixture {
    state: AppState,
    index: Index,
    store: Arc<MemoryStore>,
    admin_id: i64,
    member_id: i64,
    _dir: tempfile::TempDir,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum GateOperation {
    PutFile,
    Delete,
}

struct Gate {
    operation: GateOperation,
    key: String,
    entered: Arc<Notify>,
    release: Arc<Notify>,
}

struct GatedStore {
    inner: MemoryStore,
    gate: Mutex<Option<Gate>>,
    fail_delete_before_apply: Mutex<Option<String>>,
    fail_delete_after_apply: Mutex<Option<String>>,
}

impl GatedStore {
    fn new() -> Self {
        Self {
            inner: MemoryStore::new(),
            gate: Mutex::new(None),
            fail_delete_before_apply: Mutex::new(None),
            fail_delete_after_apply: Mutex::new(None),
        }
    }

    fn arm(&self, operation: GateOperation, key: &str) -> (Arc<Notify>, Arc<Notify>) {
        let entered = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        *self.gate.lock().unwrap() = Some(Gate {
            operation,
            key: key.to_owned(),
            entered: entered.clone(),
            release: release.clone(),
        });
        (entered, release)
    }

    fn take_gate(&self, operation: GateOperation, key: &str) -> Option<Gate> {
        let mut gate = self.gate.lock().unwrap();
        if gate
            .as_ref()
            .is_some_and(|gate| gate.operation == operation && gate.key == key)
        {
            gate.take()
        } else {
            None
        }
    }

    fn fail_delete_after_apply(&self, key: &str) {
        *self.fail_delete_after_apply.lock().unwrap() = Some(key.to_owned());
    }

    fn fail_delete_before_apply(&self, key: &str) {
        *self.fail_delete_before_apply.lock().unwrap() = Some(key.to_owned());
    }
}

#[async_trait]
impl ObjectStore for GatedStore {
    async fn list(&self, prefix: &str, token: Option<String>) -> StorageResult<ListPage> {
        self.inner.list(prefix, token).await
    }

    async fn get(&self, key: &str) -> StorageResult<Bytes> {
        self.inner.get(key).await
    }

    async fn get_range(&self, key: &str, range: Range<u64>) -> StorageResult<Bytes> {
        self.inner.get_range(key, range).await
    }

    async fn put(&self, key: &str, bytes: Bytes) -> StorageResult<ObjectMeta> {
        self.inner.put(key, bytes).await
    }

    async fn put_file(&self, key: &str, path: &Path) -> StorageResult<ObjectMeta> {
        let result = self.inner.put_file(key, path).await;
        if let Some(gate) = self.take_gate(GateOperation::PutFile, key) {
            gate.entered.notify_one();
            gate.release.notified().await;
        }
        result
    }

    async fn delete(&self, key: &str) -> StorageResult<()> {
        if self
            .fail_delete_before_apply
            .lock()
            .unwrap()
            .take_if(|failed_key| failed_key == key)
            .is_some()
        {
            return Err(StorageError::Backend(
                "delete rejected before apply".to_owned(),
            ));
        }
        let result = self.inner.delete(key).await;
        if self
            .fail_delete_after_apply
            .lock()
            .unwrap()
            .take_if(|failed_key| failed_key == key)
            .is_some()
        {
            return Err(StorageError::Backend(
                "delete response lost after apply".to_owned(),
            ));
        }
        if let Some(gate) = self.take_gate(GateOperation::Delete, key) {
            gate.entered.notify_one();
            gate.release.notified().await;
        }
        result
    }

    async fn head(&self, key: &str) -> StorageResult<ObjectMeta> {
        self.inner.head(key).await
    }
}

struct GatedFixture {
    state: AppState,
    index: Index,
    store: Arc<GatedStore>,
    _dir: tempfile::TempDir,
}

impl GatedFixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let index = Index::connect(&dir.path().join("yevune.sqlite"))
            .await
            .unwrap();
        let encryptor = Encryptor::new("pwd:test-secret");
        UserAdmin::new(&index, &encryptor)
            .create_user("admin", "secret", true)
            .await
            .unwrap();
        let store = Arc::new(GatedStore::new());
        let object_store: Arc<dyn ObjectStore> = store.clone();
        let state = AppState::new(
            index.clone(),
            object_store,
            "test-secret",
            "/missing/ffmpeg",
        );
        Self {
            state,
            index,
            store,
            _dir: dir,
        }
    }

    fn uri(&self, path: &str) -> String {
        let separator = if path.contains('?') { '&' } else { '?' };
        format!("{path}{separator}u=admin&p=secret&v=1.16.1&c=test&f=json")
    }

    async fn get(&self, path: &str) -> axum::response::Response {
        yevune_server::app(self.state.clone())
            .oneshot(
                Request::builder()
                    .uri(self.uri(path))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
    }

    async fn upload(&self, key: &str, bytes: &[u8]) -> axum::response::Response {
        yevune_server::app(self.state.clone())
            .oneshot(upload_request(
                self.uri("/rest/ext/uploadTrack"),
                key,
                bytes,
            ))
            .await
            .unwrap()
    }
}

fn upload_request(uri: String, key: &str, bytes: &[u8]) -> Request<Body> {
    let boundary = "ext-test-boundary";
    let mut body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"key\"\r\n\r\n{key}\r\n\
         --{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"track.flac\"\r\n\
         Content-Type: audio/flac\r\n\r\n"
    )
    .into_bytes();
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .unwrap()
}

fn cover_request(uri: String, id: &str, bytes: &[u8]) -> Request<Body> {
    let boundary = "cover-test-boundary";
    let mut body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"id\"\r\n\r\n{id}\r\n\
         --{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"cover.png\"\r\n\
         Content-Type: image/png\r\n\r\n"
    )
    .into_bytes();
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .unwrap()
}

async fn wait_for_track_key(index: &Index, expected: &str) {
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            let key: Option<String> = sqlx::query_scalar("SELECT object_key FROM tracks LIMIT 1")
                .fetch_optional(index.pool())
                .await
                .unwrap();
            if key.as_deref() == Some(expected) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("索引对象键应在 owned operation 完成后收敛");
}

async fn wait_for_no_tracks(index: &Index) {
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tracks")
                .fetch_one(index.pool())
                .await
                .unwrap();
            if count == 0 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("索引删除应在 owned operation 完成后收敛");
}

async fn wait_for_move_final(index: &Index, store: &GatedStore, source: &str, destination: &str) {
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            let key: Option<String> = sqlx::query_scalar("SELECT object_key FROM tracks LIMIT 1")
                .fetch_optional(index.pool())
                .await
                .unwrap();
            if key.as_deref() == Some(destination)
                && store.head(source).await.is_err()
                && store.head(destination).await.is_ok()
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("移动取消后对象与索引应完成终态收敛");
}

impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let index = Index::connect(&dir.path().join("yevune.sqlite"))
            .await
            .unwrap();
        let encryptor = Encryptor::new("pwd:test-secret");
        let users = UserAdmin::new(&index, &encryptor);
        let admin_id = users
            .create_user("admin", "secret", true)
            .await
            .unwrap()
            .id
            .parse()
            .unwrap();
        let member_id = users
            .create_user("member", "secret", false)
            .await
            .unwrap()
            .id
            .parse()
            .unwrap();
        let store = Arc::new(MemoryStore::new());
        let object_store: Arc<dyn ObjectStore> = store.clone();
        let state = AppState::new(
            index.clone(),
            object_store,
            "test-secret",
            "/missing/ffmpeg",
        );
        Self {
            state,
            index,
            store,
            admin_id,
            member_id,
            _dir: dir,
        }
    }

    fn uri(&self, user: &str, path: &str) -> String {
        let separator = if path.contains('?') { '&' } else { '?' };
        format!("{path}{separator}u={user}&p=secret&v=1.16.1&c=test&f=json")
    }

    async fn get(&self, user: &str, path: &str) -> axum::response::Response {
        yevune_server::app(self.state.clone())
            .oneshot(
                Request::builder()
                    .uri(self.uri(user, path))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
    }

    async fn upload(&self, key: &str, bytes: &[u8]) -> axum::response::Response {
        yevune_server::app(self.state.clone())
            .oneshot(upload_request(
                self.uri("admin", "/rest/ext/uploadTrack"),
                key,
                bytes,
            ))
            .await
            .unwrap()
    }
}

async fn json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

fn payload<'a>(body: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
    &body["subsonic-response"][name]
}

#[tokio::test]
async fn library_management_rejects_non_admin() {
    let fixture = Fixture::new().await;
    let response = fixture.get("member", "/rest/ext/deleteTrack?id=tr-1").await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    assert_eq!(body["subsonic-response"]["status"], "failed");
    assert_eq!(body["subsonic-response"]["error"]["code"], 50);
}

#[tokio::test]
async fn playlist_tree_crud_move_and_cross_owner_rejection() {
    let fixture = Fixture::new().await;

    let created = json(
        fixture
            .get("member", "/rest/ext/createPlaylistFolder?name=Chinese")
            .await,
    )
    .await;
    let folder_id = payload(&created, "playlistFolder")["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        payload(&created, "playlistFolder")["ownerId"],
        format!("us-{}", fixture.member_id)
    );

    let renamed = json(
        fixture
            .get(
                "member",
                &format!("/rest/ext/updatePlaylistFolder?id={folder_id}&name=Mandarin"),
            )
            .await,
    )
    .await;
    assert_eq!(renamed["subsonic-response"]["status"], "ok");

    let playlist_id = fixture
        .index
        .playlists()
        .create_playlist(fixture.member_id, "Hits", None)
        .await
        .unwrap();
    let moved = json(
        fixture
            .get(
                "member",
                &format!("/rest/ext/movePlaylist?id=pl-{playlist_id}&folderId={folder_id}"),
            )
            .await,
    )
    .await;
    assert_eq!(moved["subsonic-response"]["status"], "ok");

    let tree = json(fixture.get("member", "/rest/ext/getPlaylistTree").await).await;
    assert_eq!(
        payload(&tree, "playlistTree")["folders"][0]["name"],
        "Mandarin"
    );
    assert_eq!(
        payload(&tree, "playlistTree")["playlists"][0]["folderId"],
        folder_id
    );

    let child = json(
        fixture
            .get(
                "member",
                &format!("/rest/ext/createPlaylistFolder?name=Child&parentId={folder_id}"),
            )
            .await,
    )
    .await;
    let child_id = payload(&child, "playlistFolder")["id"].as_str().unwrap();
    let cycle = json(
        fixture
            .get(
                "member",
                &format!("/rest/ext/moveFolder?id={folder_id}&parentId={child_id}"),
            )
            .await,
    )
    .await;
    assert_eq!(cycle["subsonic-response"]["error"]["code"], 10);

    let forbidden = json(
        fixture
            .get(
                "admin",
                &format!("/rest/ext/updatePlaylistFolder?id={folder_id}&name=Stolen"),
            )
            .await,
    )
    .await;
    assert_eq!(forbidden["subsonic-response"]["error"]["code"], 70);

    let deleted = json(
        fixture
            .get(
                "member",
                &format!("/rest/ext/deletePlaylistFolder?id={folder_id}"),
            )
            .await,
    )
    .await;
    assert_eq!(deleted["subsonic-response"]["status"], "ok");
}

fn flac(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/tests/fixtures/scanner/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

#[tokio::test]
async fn upload_streams_to_store_and_scanner_indexes_track() {
    let fixture = Fixture::new().await;
    let response = fixture
        .upload("library/uploads/a.flac", &flac("a.flac"))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    let id = payload(&body, "track")["id"].as_str().unwrap();
    assert!(id.starts_with("tr-"), "{body}");
    assert!(fixture.store.head("library/uploads/a.flac").await.is_ok());
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tracks WHERE object_key = 'library/uploads/a.flac'",
    )
    .fetch_one(fixture.index.pool())
    .await
    .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn update_tags_is_override_only_and_write_back_is_explicit() {
    let fixture = Fixture::new().await;
    let uploaded = json(
        fixture
            .upload("library/uploads/tags.flac", &flac("a.flac"))
            .await,
    )
    .await;
    let id = payload(&uploaded, "track")["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let before = fixture
        .store
        .head("library/uploads/tags.flac")
        .await
        .unwrap();

    let updated = json(
        fixture
            .get(
                "admin",
                &format!("/rest/ext/updateTags?id={id}&title=Override%20Title&genre=Jazz"),
            )
            .await,
    )
    .await;
    assert_eq!(updated["subsonic-response"]["status"], "ok");
    assert_eq!(
        fixture
            .store
            .head("library/uploads/tags.flac")
            .await
            .unwrap(),
        before
    );

    let shown = json(
        fixture
            .get("admin", &format!("/rest/getSong?id={id}"))
            .await,
    )
    .await;
    assert_eq!(payload(&shown, "song")["title"], "Override Title");
    assert_eq!(payload(&shown, "song")["genre"], "Jazz");
    let genres = json(fixture.get("admin", "/rest/getGenres").await).await;
    assert!(
        payload(&genres, "genres")["genre"]
            .as_array()
            .unwrap()
            .iter()
            .any(|genre| genre["value"] == "Jazz"),
        "{genres}"
    );

    let written = json(
        fixture
            .get(
                "admin",
                &format!("/rest/ext/writeBackTags?id={id}&title=Written%20Title"),
            )
            .await,
    )
    .await;
    assert_eq!(written["subsonic-response"]["status"], "ok", "{written}");
    assert_ne!(
        fixture
            .store
            .head("library/uploads/tags.flac")
            .await
            .unwrap(),
        before
    );
    let raw_title: String = sqlx::query_scalar(
        "SELECT title FROM tracks WHERE object_key = 'library/uploads/tags.flac'",
    )
    .fetch_one(fixture.index.pool())
    .await
    .unwrap();
    assert_eq!(raw_title, "Written Title");
    let stored_etag: Option<String> = sqlx::query_scalar(
        "SELECT etag FROM tracks WHERE object_key = 'library/uploads/tags.flac'",
    )
    .fetch_one(fixture.index.pool())
    .await
    .unwrap();
    assert_eq!(
        stored_etag,
        fixture
            .store
            .head("library/uploads/tags.flac")
            .await
            .unwrap()
            .etag
    );
}

#[tokio::test]
async fn move_and_delete_track_keep_store_and_index_consistent() {
    let fixture = Fixture::new().await;
    let uploaded = json(
        fixture
            .upload("library/uploads/source.flac", &flac("a.flac"))
            .await,
    )
    .await;
    let id = payload(&uploaded, "track")["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let moved = json(
        fixture
            .get(
                "admin",
                &format!("/rest/ext/moveTrack?id={id}&key=library/organized/moved.flac"),
            )
            .await,
    )
    .await;
    assert_eq!(moved["subsonic-response"]["status"], "ok");
    assert!(fixture
        .store
        .head("library/uploads/source.flac")
        .await
        .is_err());
    assert!(fixture
        .store
        .head("library/organized/moved.flac")
        .await
        .is_ok());
    let key: String = sqlx::query_scalar("SELECT object_key FROM tracks")
        .fetch_one(fixture.index.pool())
        .await
        .unwrap();
    assert_eq!(key, "library/organized/moved.flac");

    let deleted = json(
        fixture
            .get("admin", &format!("/rest/ext/deleteTrack?id={id}"))
            .await,
    )
    .await;
    assert_eq!(deleted["subsonic-response"]["status"], "ok");
    assert!(fixture
        .store
        .head("library/organized/moved.flac")
        .await
        .is_err());
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tracks")
        .fetch_one(fixture.index.pool())
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn access_rule_allowlist_round_trips_and_is_admin_only() {
    let fixture = Fixture::new().await;
    let seeded = json(
        fixture
            .upload("library/access/rock.flac", &flac("a.flac"))
            .await,
    )
    .await;
    assert_eq!(seeded["subsonic-response"]["status"], "ok");
    let role_id = fixture
        .index
        .roles()
        .create_role("kids", false)
        .await
        .unwrap();
    let set = json(
        fixture
            .get(
                "admin",
                &format!(
                    "/rest/ext/setAccessRule?scopeType=genre&scopeId=Rock&grant=user:us-{}&grant=role:ro-{role_id}",
                    fixture.member_id
                ),
            )
            .await,
    )
    .await;
    assert_eq!(set["subsonic-response"]["status"], "ok", "{set}");
    let rule_id = payload(&set, "accessRule")["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let created_by: i64 = sqlx::query_scalar("SELECT created_by FROM access_rules")
        .fetch_one(fixture.index.pool())
        .await
        .unwrap();
    assert_eq!(created_by, fixture.admin_id);

    let listed = json(fixture.get("admin", "/rest/ext/getAccessRules").await).await;
    let grants = payload(&listed, "accessRules")["accessRule"][0]["grants"]
        .as_array()
        .unwrap();
    assert_eq!(grants.len(), 2, "{listed}");
    assert!(grants
        .iter()
        .any(|grant| grant["id"] == format!("us-{}", fixture.member_id)));
    assert!(grants
        .iter()
        .any(|grant| grant["id"] == format!("ro-{role_id}")));

    let denied = json(fixture.get("member", "/rest/ext/getAccessRules").await).await;
    assert_eq!(denied["subsonic-response"]["error"]["code"], 50);

    let deleted = json(
        fixture
            .get("admin", &format!("/rest/ext/deleteAccessRule?id={rule_id}"))
            .await,
    )
    .await;
    assert_eq!(deleted["subsonic-response"]["status"], "ok");
}

#[tokio::test]
async fn duplicate_access_grants_are_idempotent_in_response() {
    let fixture = Fixture::new().await;
    let uploaded = json(
        fixture
            .upload("library/access/duplicates.flac", &flac("a.flac"))
            .await,
    )
    .await;
    assert_eq!(uploaded["subsonic-response"]["status"], "ok");
    let role_id = fixture
        .index
        .roles()
        .create_role("duplicate-role", false)
        .await
        .unwrap();
    let baseline = json(
        fixture
            .get(
                "admin",
                &format!(
                    "/rest/ext/setAccessRule?scopeType=genre&scopeId=Rock&grant=user:us-{0}&grant=role:ro-{role_id}",
                    fixture.member_id
                ),
            )
            .await,
    )
    .await;
    assert_eq!(baseline["subsonic-response"]["status"], "ok", "{baseline}");
    let baseline_grants = payload(&baseline, "accessRule")["grants"].clone();
    let body = json(
        fixture
            .get(
                "admin",
                &format!(
                    "/rest/ext/setAccessRule?scopeType=genre&scopeId=Rock&grant=user:us-{0}&grant=role:ro-{role_id}&grant=user:us-{0}&grant=role:ro-{role_id}",
                    fixture.member_id
                ),
            )
            .await,
    )
    .await;
    assert_eq!(body["subsonic-response"]["status"], "ok", "{body}");
    let grants = payload(&body, "accessRule")["grants"].as_array().unwrap();
    assert_eq!(grants.len(), 2, "{body}");
    assert_eq!(payload(&body, "accessRule")["grants"], baseline_grants);
    assert_eq!(
        grants
            .iter()
            .filter(|grant| grant["id"] == format!("us-{}", fixture.member_id))
            .count(),
        1
    );
    assert_eq!(
        grants
            .iter()
            .filter(|grant| grant["id"] == format!("ro-{role_id}"))
            .count(),
        1
    );
}

#[tokio::test]
async fn deleted_principals_map_to_not_found_without_orphans() {
    let fixture = Fixture::new().await;
    let uploaded = json(
        fixture
            .upload("library/access/race.flac", &flac("a.flac"))
            .await,
    )
    .await;
    assert_eq!(uploaded["subsonic-response"]["status"], "ok");
    let role_id = fixture
        .index
        .roles()
        .create_role("racing-role", false)
        .await
        .unwrap();

    for (principal_type, prefix, table, principal_id) in [
        ("user", "us", "users", fixture.member_id),
        ("role", "ro", "roles", role_id),
    ] {
        sqlx::query(&format!("DELETE FROM {table} WHERE id = ?"))
            .bind(principal_id)
            .execute(fixture.index.pool())
            .await
            .unwrap();

        let body = json(
            fixture
                .get(
                    "admin",
                    &format!(
                        "/rest/ext/setAccessRule?scopeType=genre&scopeId=Rock&grant={principal_type}:{prefix}-{principal_id}"
                    ),
                )
                .await,
        )
        .await;
        assert_eq!(body["subsonic-response"]["error"]["code"], 70, "{body}");
        let rule_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM access_rules")
            .fetch_one(fixture.index.pool())
            .await
            .unwrap();
        let orphan_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM access_rule_grants WHERE principal_type = ? AND principal_id = ?",
        )
        .bind(principal_type)
        .bind(principal_id)
        .fetch_one(fixture.index.pool())
        .await
        .unwrap();
        assert_eq!(rule_count, 0);
        assert_eq!(orphan_count, 0);
    }
}

#[tokio::test]
async fn deleting_principals_cleans_access_grants() {
    let fixture = Fixture::new().await;
    let uploaded = json(
        fixture
            .upload("library/access/member.flac", &flac("a.flac"))
            .await,
    )
    .await;
    assert_eq!(uploaded["subsonic-response"]["status"], "ok");
    let set = json(
        fixture
            .get(
                "admin",
                &format!(
                    "/rest/ext/setAccessRule?scopeType=genre&scopeId=Rock&grant=user:us-{}",
                    fixture.member_id
                ),
            )
            .await,
    )
    .await;
    assert_eq!(set["subsonic-response"]["status"], "ok", "{set}");

    let deleted = json(
        fixture
            .get("admin", "/rest/deleteUser?username=member")
            .await,
    )
    .await;
    assert_eq!(deleted["subsonic-response"]["status"], "ok", "{deleted}");
    let user_grants: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM access_rule_grants WHERE principal_type = ? AND principal_id = ?",
    )
    .bind("user")
    .bind(fixture.member_id)
    .fetch_one(fixture.index.pool())
    .await
    .unwrap();
    let user_remaining_rules: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM access_rules")
        .fetch_one(fixture.index.pool())
        .await
        .unwrap();

    let fixture = Fixture::new().await;
    let uploaded = json(
        fixture
            .upload("library/access/role.flac", &flac("b.flac"))
            .await,
    )
    .await;
    assert_eq!(uploaded["subsonic-response"]["status"], "ok");
    let created = json(
        fixture
            .get("admin", "/rest/ext/createRole?name=listener")
            .await,
    )
    .await;
    let role_id = payload(&created, "role")["id"].as_str().unwrap().to_owned();
    let role_id_number = role_id.trim_start_matches("ro-").parse::<i64>().unwrap();
    let set = json(
        fixture
            .get(
                "admin",
                &format!(
                    "/rest/ext/setAccessRule?scopeType=genre&scopeId=Jazz&grant=role:{role_id}"
                ),
            )
            .await,
    )
    .await;
    assert_eq!(set["subsonic-response"]["status"], "ok", "{set}");

    let deleted = json(
        fixture
            .get("admin", &format!("/rest/ext/deleteRole?id={role_id}"))
            .await,
    )
    .await;
    assert_eq!(deleted["subsonic-response"]["status"], "ok", "{deleted}");
    let role_grants: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM access_rule_grants WHERE principal_type = ? AND principal_id = ?",
    )
    .bind("role")
    .bind(role_id_number)
    .fetch_one(fixture.index.pool())
    .await
    .unwrap();
    let role_remaining_rules: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM access_rules")
        .fetch_one(fixture.index.pool())
        .await
        .unwrap();

    assert_eq!(
        user_grants, 0,
        "删除用户后应清理 user grant；删除角色后的 role grant 数为 {role_grants}"
    );
    assert_eq!(role_grants, 0, "删除角色后应清理 role grant");
    assert_eq!(user_remaining_rules, 1, "规则保留并收敛为仅管理员可见");
    assert_eq!(role_remaining_rules, 1, "规则保留并收敛为仅管理员可见");
}

#[tokio::test]
async fn access_rules_include_scope_display_names() {
    let fixture = Fixture::new().await;
    let uploaded = json(
        fixture
            .upload("library/access/names.flac", &flac("a.flac"))
            .await,
    )
    .await;
    let track = payload(&uploaded, "track");
    let track_id = track["id"].as_str().unwrap().to_owned();
    let album_id = track["albumId"].as_str().unwrap().to_owned();
    let artist_id = track["artistId"].as_str().unwrap().to_owned();

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
        assert_eq!(
            payload(&body, "accessRule")["scopeName"],
            expected_name,
            "{body}"
        );
    }

    let listed = json(fixture.get("admin", "/rest/ext/getAccessRules").await).await;
    let rules = payload(&listed, "accessRules")["accessRule"]
        .as_array()
        .unwrap();
    assert_eq!(rules.len(), 4);
    assert!(
        rules
            .iter()
            .all(|rule| rule["scopeName"].as_str().is_some()),
        "{listed}"
    );
}

#[tokio::test]
async fn role_crud_assign_and_unassign_are_admin_only() {
    let fixture = Fixture::new().await;
    let created = json(
        fixture
            .get("admin", "/rest/ext/createRole?name=listener")
            .await,
    )
    .await;
    let role_id = payload(&created, "role")["id"].as_str().unwrap().to_owned();
    assert!(role_id.starts_with("ro-"), "{created}");

    for endpoint in ["assignRole", "unassignRole"] {
        let result = json(
            fixture
                .get(
                    "admin",
                    &format!(
                        "/rest/ext/{endpoint}?userId=us-{}&roleId={role_id}",
                        fixture.member_id
                    ),
                )
                .await,
        )
        .await;
        assert_eq!(result["subsonic-response"]["status"], "ok", "{result}");
    }
    let assigned: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM user_roles WHERE user_id = ? AND role_id = ?")
            .bind(fixture.member_id)
            .bind(role_id.trim_start_matches("ro-").parse::<i64>().unwrap())
            .fetch_one(fixture.index.pool())
            .await
            .unwrap();
    assert_eq!(assigned, 0);

    let roles = json(fixture.get("admin", "/rest/ext/getRoles").await).await;
    assert!(payload(&roles, "roles")["role"]
        .as_array()
        .unwrap()
        .iter()
        .any(|role| role["name"] == "listener"));
    let deleted = json(
        fixture
            .get("admin", &format!("/rest/ext/deleteRole?id={role_id}"))
            .await,
    )
    .await;
    assert_eq!(deleted["subsonic-response"]["status"], "ok");

    let denied = json(fixture.get("member", "/rest/ext/getRoles").await).await;
    assert_eq!(denied["subsonic-response"]["error"]["code"], 50);
}

#[tokio::test]
async fn prefix_scan_only_indexes_requested_range() {
    let fixture = Fixture::new().await;
    fixture
        .store
        .put("family/a.flac", flac("a.flac").into())
        .await
        .unwrap();
    fixture
        .store
        .put("other/b.flac", flac("b.flac").into())
        .await
        .unwrap();
    let started = json(
        fixture
            .get("admin", "/rest/ext/startScan?prefix=family/")
            .await,
    )
    .await;
    assert_eq!(started["subsonic-response"]["status"], "ok", "{started}");
    let changes = payload(&started, "scanResult")["changes"]
        .as_array()
        .unwrap();
    assert_eq!(changes.len(), 1, "{started}");
    assert_eq!(changes[0]["action"], "added");
    assert_eq!(changes[0]["objectKey"], "family/a.flac");
    assert_eq!(changes[0]["track"]["title"], "Song A");
    assert_eq!(payload(&started, "scanResult")["changesTruncated"], false);
    let keys: Vec<String> = sqlx::query_scalar("SELECT object_key FROM tracks ORDER BY object_key")
        .fetch_all(fixture.index.pool())
        .await
        .unwrap();
    assert_eq!(keys, vec!["family/a.flac"]);
}

#[tokio::test]
async fn admin_get_users_extension_returns_ids_and_custom_roles() {
    let fixture = Fixture::new().await;
    fixture
        .index
        .users()
        .set_email(fixture.member_id, Some("member@family.example"))
        .await
        .unwrap();
    let family_role = fixture
        .index
        .roles()
        .create_role("family", false)
        .await
        .unwrap();
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
    assert_eq!(member["email"], "member@family.example");
    assert!(member["created"]
        .as_str()
        .is_some_and(|value| !value.is_empty()));
    assert_eq!(member["admin"], false);
    assert!(member["roles"]
        .as_array()
        .unwrap()
        .contains(&serde_json::json!("member")));
    assert!(member["roles"]
        .as_array()
        .unwrap()
        .contains(&serde_json::json!("family")));
}

#[tokio::test]
async fn member_cannot_list_users_through_extension() {
    let fixture = Fixture::new().await;
    let body = json(fixture.get("member", "/rest/ext/getUsers").await).await;

    assert_eq!(body["subsonic-response"]["error"]["code"], 50);
}

#[tokio::test]
async fn extensions_discovery_declares_every_ext_capability() {
    let fixture = Fixture::new().await;
    let body = json(
        fixture
            .get("admin", "/rest/getOpenSubsonicExtensions")
            .await,
    )
    .await;
    let names: Vec<_> = payload(&body, "openSubsonicExtensions")
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|value| value["name"].as_str())
        .collect();
    for name in [
        "playlistTree",
        "libraryManagement",
        "accessControl",
        "roleManagement",
        "userManagement",
        "prefixScan",
        "coverArtManagement",
    ] {
        assert!(names.contains(&name), "缺少 {name}: {body}");
    }
}

#[tokio::test]
async fn set_cover_art_streams_image_and_associates_album() {
    let fixture = Fixture::new().await;
    let album_id: i64 =
        sqlx::query_scalar("INSERT INTO albums(name) VALUES('Replacement') RETURNING id")
            .fetch_one(fixture.index.pool())
            .await
            .unwrap();
    let bytes = b"replacement-cover";
    let response = yevune_server::app(fixture.state.clone())
        .oneshot(cover_request(
            fixture.uri("admin", "/rest/ext/setCoverArt"),
            &format!("al-{album_id}"),
            bytes,
        ))
        .await
        .unwrap();
    let body = json(response).await;
    assert_eq!(body["subsonic-response"]["status"], "ok", "{body}");
    let key: String = sqlx::query_scalar("SELECT cover_key FROM albums WHERE id = ?")
        .bind(album_id)
        .fetch_one(fixture.index.pool())
        .await
        .unwrap();
    assert!(key.starts_with("covers/"));
    assert_eq!(fixture.store.get(&key).await.unwrap().as_ref(), bytes);
}

#[tokio::test]
async fn failed_upload_leaves_no_object_or_index_row() {
    let fixture = Fixture::new().await;
    let response = fixture
        .upload("library/uploads/bad.flac", b"not audio")
        .await;
    let body = json(response).await;
    assert_eq!(body["subsonic-response"]["status"], "failed");
    assert!(fixture
        .store
        .head("library/uploads/bad.flac")
        .await
        .is_err());
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tracks")
        .fetch_one(fixture.index.pool())
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn failed_replacement_upload_preserves_previous_object_and_index() {
    let fixture = Fixture::new().await;
    let first = json(
        fixture
            .upload("library/uploads/existing.flac", &flac("a.flac"))
            .await,
    )
    .await;
    assert_eq!(first["subsonic-response"]["status"], "ok");
    let previous = fixture
        .store
        .get("library/uploads/existing.flac")
        .await
        .unwrap();
    let previous_etag: Option<String> = sqlx::query_scalar(
        "SELECT etag FROM tracks WHERE object_key = 'library/uploads/existing.flac'",
    )
    .fetch_one(fixture.index.pool())
    .await
    .unwrap();

    let failed = json(
        fixture
            .upload("library/uploads/existing.flac", b"not audio")
            .await,
    )
    .await;
    assert_eq!(failed["subsonic-response"]["status"], "failed");
    assert_eq!(
        fixture
            .store
            .get("library/uploads/existing.flac")
            .await
            .unwrap(),
        previous
    );
    let current_etag: Option<String> = sqlx::query_scalar(
        "SELECT etag FROM tracks WHERE object_key = 'library/uploads/existing.flac'",
    )
    .fetch_one(fixture.index.pool())
    .await
    .unwrap();
    assert_eq!(current_etag, previous_etag);
}

#[tokio::test]
async fn cancelled_upload_leaves_no_object_or_index() {
    let fixture = Fixture::new().await;
    let boundary = "cancel-boundary";
    let prefix = Bytes::from(format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"key\"\r\n\r\nlibrary/uploads/cancel.flac\r\n\
         --{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"track.flac\"\r\n\
         Content-Type: audio/flac\r\n\r\npartial"
    ));
    let body_stream =
        stream::once(async move { Ok::<_, std::io::Error>(prefix) }).chain(stream::pending());
    let app = yevune_server::app(fixture.state.clone());
    let request = Request::builder()
        .method(Method::POST)
        .uri(fixture.uri("admin", "/rest/ext/uploadTrack"))
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from_stream(body_stream))
        .unwrap();
    let task = tokio::spawn(async move { app.oneshot(request).await });
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    task.abort();
    let _ = task.await;

    assert!(fixture
        .store
        .head("library/uploads/cancel.flac")
        .await
        .is_err());
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tracks")
        .fetch_one(fixture.index.pool())
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn tag_override_drives_search_matching_and_empty_search_order() {
    let fixture = Fixture::new().await;
    let first = json(
        fixture
            .upload("library/search/a.flac", &flac("a.flac"))
            .await,
    )
    .await;
    let second = json(
        fixture
            .upload("library/search/b.flac", &flac("b.flac"))
            .await,
    )
    .await;
    let first_id = payload(&first, "track")["id"].as_str().unwrap();
    let second_id = payload(&second, "track")["id"].as_str().unwrap();
    let _ = fixture
        .get(
            "admin",
            &format!("/rest/ext/updateTags?id={first_id}&title=Zulu"),
        )
        .await;
    let _ = fixture
        .get(
            "admin",
            &format!("/rest/ext/updateTags?id={second_id}&title=Aardvark"),
        )
        .await;

    let old = json(fixture.get("admin", "/rest/search3?query=Song%20A").await).await;
    assert!(
        payload(&old, "searchResult3")["song"]
            .as_array()
            .unwrap()
            .is_empty(),
        "{old}"
    );
    let new = json(fixture.get("admin", "/rest/search3?query=Zulu").await).await;
    assert_eq!(
        payload(&new, "searchResult3")["song"][0]["title"],
        "Zulu",
        "{new}"
    );
    let empty = json(fixture.get("admin", "/rest/search3?query=").await).await;
    let songs = payload(&empty, "searchResult3")["song"].as_array().unwrap();
    assert_eq!(songs[0]["title"], "Aardvark", "{empty}");
    assert_eq!(songs[1]["title"], "Zulu", "{empty}");
}

#[tokio::test]
async fn write_back_clears_only_written_overrides_after_success() {
    let fixture = Fixture::new().await;
    let uploaded = json(
        fixture
            .upload("library/writeback/a.flac", &flac("a.flac"))
            .await,
    )
    .await;
    let id = payload(&uploaded, "track")["id"].as_str().unwrap();
    let _ = fixture
        .get(
            "admin",
            &format!("/rest/ext/updateTags?id={id}&title=Override&genre=Jazz"),
        )
        .await;
    let written = json(
        fixture
            .get(
                "admin",
                &format!("/rest/ext/writeBackTags?id={id}&title=Written"),
            )
            .await,
    )
    .await;
    assert_eq!(written["subsonic-response"]["status"], "ok", "{written}");
    let shown = json(
        fixture
            .get("admin", &format!("/rest/getSong?id={id}"))
            .await,
    )
    .await;
    assert_eq!(payload(&shown, "song")["title"], "Written", "{shown}");
    assert_eq!(payload(&shown, "song")["genre"], "Jazz", "{shown}");
    let fields: Vec<String> = sqlx::query_scalar("SELECT field FROM tag_overrides ORDER BY field")
        .fetch_all(fixture.index.pool())
        .await
        .unwrap();
    assert_eq!(fields, vec!["genre"]);
}

#[tokio::test]
async fn access_grants_use_form_decoding_and_validate_scope_exists() {
    let fixture = Fixture::new().await;
    let seeded = json(
        fixture
            .upload("library/access/encoded.flac", &flac("a.flac"))
            .await,
    )
    .await;
    assert_eq!(seeded["subsonic-response"]["status"], "ok");
    let role_id = fixture
        .index
        .roles()
        .create_role("kids", false)
        .await
        .unwrap();
    let encoded = json(
        fixture
            .get(
                "admin",
                &format!(
                    "/rest/ext/setAccessRule?scopeType=genre&scopeId=Rock&grant=user%3Aus-{}&grant=role%3Aro-{role_id}",
                    fixture.member_id
                ),
            )
            .await,
    )
    .await;
    assert_eq!(encoded["subsonic-response"]["status"], "ok", "{encoded}");

    for path in [
        "/rest/ext/setAccessRule?scopeType=track&scopeId=tr-999999",
        "/rest/ext/setAccessRule?scopeType=genre&scopeId=Missing+Genre",
    ] {
        let missing = json(fixture.get("admin", path).await).await;
        assert_eq!(
            missing["subsonic-response"]["error"]["code"], 70,
            "{missing}"
        );
    }
}

#[tokio::test]
async fn numeric_tag_fields_reject_malformed_or_out_of_range_values() {
    let fixture = Fixture::new().await;
    let uploaded = json(
        fixture
            .upload("library/validation/a.flac", &flac("a.flac"))
            .await,
    )
    .await;
    let id = payload(&uploaded, "track")["id"].as_str().unwrap();
    for query in ["year=abc", "year=10000", "track=0", "discNumber=1000"] {
        let body = json(
            fixture
                .get("admin", &format!("/rest/ext/updateTags?id={id}&{query}"))
                .await,
        )
        .await;
        assert_eq!(
            body["subsonic-response"]["error"]["code"], 10,
            "{query}: {body}"
        );
    }
    let empty_write_back = json(
        fixture
            .get("admin", &format!("/rest/ext/writeBackTags?id={id}"))
            .await,
    )
    .await;
    assert_eq!(
        empty_write_back["subsonic-response"]["error"]["code"], 10,
        "{empty_write_back}"
    );
}

#[tokio::test]
async fn track_and_disc_overrides_drive_album_song_order() {
    let fixture = Fixture::new().await;
    let first = json(
        fixture
            .upload("library/order/a.flac", &flac("a.flac"))
            .await,
    )
    .await;
    let second = json(
        fixture
            .upload("library/order/b.flac", &flac("b.flac"))
            .await,
    )
    .await;
    let first_track = payload(&first, "track");
    let second_track = payload(&second, "track");
    let first_id = first_track["id"].as_str().unwrap();
    let second_id = second_track["id"].as_str().unwrap();
    let album_id = first_track["albumId"].as_str().unwrap();
    sqlx::query("UPDATE tracks SET album_id = ? WHERE id = ?")
        .bind(album_id.trim_start_matches("al-").parse::<i64>().unwrap())
        .bind(second_id.trim_start_matches("tr-").parse::<i64>().unwrap())
        .execute(fixture.index.pool())
        .await
        .unwrap();
    let _ = fixture
        .get(
            "admin",
            &format!("/rest/ext/updateTags?id={first_id}&discNumber=1&track=2&title=Aardvark"),
        )
        .await;
    let _ = fixture
        .get(
            "admin",
            &format!("/rest/ext/updateTags?id={second_id}&discNumber=1&track=1&title=Zulu"),
        )
        .await;

    let album = json(
        fixture
            .get("admin", &format!("/rest/getAlbum?id={album_id}"))
            .await,
    )
    .await;
    assert_eq!(payload(&album, "album")["song"][0]["title"], "Zulu");
    assert_eq!(payload(&album, "album")["song"][1]["title"], "Aardvark");
}

#[tokio::test]
async fn cancelled_visible_upload_finishes_index_commit() {
    let fixture = GatedFixture::new().await;
    let (entered, release) = fixture
        .store
        .arm(GateOperation::PutFile, "library/cancel/upload.flac");
    let app = yevune_server::app(fixture.state.clone());
    let request = upload_request(
        fixture.uri("/rest/ext/uploadTrack"),
        "library/cancel/upload.flac",
        &flac("a.flac"),
    );
    let handler = tokio::spawn(async move { app.oneshot(request).await });
    entered.notified().await;
    handler.abort();
    let _ = handler.await;
    release.notify_one();

    wait_for_track_key(&fixture.index, "library/cancel/upload.flac").await;
    assert!(fixture
        .store
        .head("library/cancel/upload.flac")
        .await
        .is_ok());
}

#[tokio::test]
async fn cancelled_visible_write_back_finishes_scan_and_override_commit() {
    let fixture = GatedFixture::new().await;
    let uploaded = json(
        fixture
            .upload("library/cancel/write.flac", &flac("a.flac"))
            .await,
    )
    .await;
    let id = payload(&uploaded, "track")["id"].as_str().unwrap();
    let updated = json(
        fixture
            .get(&format!("/rest/ext/updateTags?id={id}&title=Override"))
            .await,
    )
    .await;
    assert_eq!(updated["subsonic-response"]["status"], "ok");

    let (entered, release) = fixture
        .store
        .arm(GateOperation::PutFile, "library/cancel/write.flac");
    let app = yevune_server::app(fixture.state.clone());
    let request = Request::builder()
        .uri(fixture.uri(&format!("/rest/ext/writeBackTags?id={id}&title=Written")))
        .body(Body::empty())
        .unwrap();
    let handler = tokio::spawn(async move { app.oneshot(request).await });
    entered.notified().await;
    handler.abort();
    let _ = handler.await;
    release.notify_one();

    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            let raw: String = sqlx::query_scalar("SELECT title FROM tracks")
                .fetch_one(fixture.index.pool())
                .await
                .unwrap();
            let overrides: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM tag_overrides WHERE field = 'title'")
                    .fetch_one(fixture.index.pool())
                    .await
                    .unwrap();
            if raw == "Written" && overrides == 0 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("写回取消后应完成扫描与覆盖层提交");
}

#[tokio::test]
async fn cancelled_visible_delete_finishes_index_commit() {
    let fixture = GatedFixture::new().await;
    let uploaded = json(
        fixture
            .upload("library/cancel/delete.flac", &flac("a.flac"))
            .await,
    )
    .await;
    let id = payload(&uploaded, "track")["id"].as_str().unwrap();
    let (entered, release) = fixture
        .store
        .arm(GateOperation::Delete, "library/cancel/delete.flac");
    let app = yevune_server::app(fixture.state.clone());
    let request = Request::builder()
        .uri(fixture.uri(&format!("/rest/ext/deleteTrack?id={id}")))
        .body(Body::empty())
        .unwrap();
    let handler = tokio::spawn(async move { app.oneshot(request).await });
    entered.notified().await;
    handler.abort();
    let _ = handler.await;
    release.notify_one();

    wait_for_no_tracks(&fixture.index).await;
    assert!(fixture
        .store
        .head("library/cancel/delete.flac")
        .await
        .is_err());
}

#[tokio::test]
async fn cancelled_visible_move_finishes_index_and_source_cleanup() {
    let fixture = GatedFixture::new().await;
    let uploaded = json(
        fixture
            .upload("library/cancel/source.flac", &flac("a.flac"))
            .await,
    )
    .await;
    let id = payload(&uploaded, "track")["id"].as_str().unwrap();
    let (entered, release) = fixture
        .store
        .arm(GateOperation::PutFile, "library/cancel/moved.flac");
    let app = yevune_server::app(fixture.state.clone());
    let request = Request::builder()
        .uri(fixture.uri(&format!(
            "/rest/ext/moveTrack?id={id}&key=library/cancel/moved.flac"
        )))
        .body(Body::empty())
        .unwrap();
    let handler = tokio::spawn(async move { app.oneshot(request).await });
    entered.notified().await;
    handler.abort();
    let _ = handler.await;
    release.notify_one();

    wait_for_move_final(
        &fixture.index,
        &fixture.store,
        "library/cancel/source.flac",
        "library/cancel/moved.flac",
    )
    .await;
}

#[tokio::test]
async fn concurrent_moves_to_same_destination_do_not_overwrite_or_delete_winner() {
    let fixture = GatedFixture::new().await;
    let first = json(
        fixture
            .upload("library/race/one.flac", &flac("a.flac"))
            .await,
    )
    .await;
    let second = json(
        fixture
            .upload("library/race/two.flac", &flac("b.flac"))
            .await,
    )
    .await;
    let first_id = payload(&first, "track")["id"].as_str().unwrap();
    let second_id = payload(&second, "track")["id"].as_str().unwrap();
    let (entered, release) = fixture
        .store
        .arm(GateOperation::PutFile, "library/race/shared.flac");

    let app = yevune_server::app(fixture.state.clone());
    let first_request = Request::builder()
        .uri(fixture.uri(&format!(
            "/rest/ext/moveTrack?id={first_id}&key=library/race/shared.flac"
        )))
        .body(Body::empty())
        .unwrap();
    let first_move = tokio::spawn(async move { app.oneshot(first_request).await.unwrap() });
    entered.notified().await;
    let app = yevune_server::app(fixture.state.clone());
    let second_request = Request::builder()
        .uri(fixture.uri(&format!(
            "/rest/ext/moveTrack?id={second_id}&key=library/race/shared.flac"
        )))
        .body(Body::empty())
        .unwrap();
    let second_move = tokio::spawn(async move { app.oneshot(second_request).await.unwrap() });
    release.notify_one();
    let first_body = json(first_move.await.unwrap()).await;
    let second_body = json(second_move.await.unwrap()).await;

    assert_eq!(
        first_body["subsonic-response"]["status"], "ok",
        "{first_body}"
    );
    assert_eq!(
        second_body["subsonic-response"]["error"]["code"], 10,
        "{second_body}"
    );
    let keys: Vec<String> = sqlx::query_scalar("SELECT object_key FROM tracks ORDER BY object_key")
        .fetch_all(fixture.index.pool())
        .await
        .unwrap();
    assert_eq!(
        keys,
        vec!["library/race/shared.flac", "library/race/two.flac"]
    );
    assert!(fixture.store.head("library/race/shared.flac").await.is_ok());
    assert!(fixture.store.head("library/race/one.flac").await.is_err());
    assert!(fixture.store.head("library/race/two.flac").await.is_ok());
}

#[tokio::test]
async fn concurrent_moves_of_same_track_serialize_and_leave_no_orphan() {
    let fixture = GatedFixture::new().await;
    let uploaded = json(
        fixture
            .upload("library/race/source.flac", &flac("a.flac"))
            .await,
    )
    .await;
    let id = payload(&uploaded, "track")["id"].as_str().unwrap();
    let (entered, release) = fixture
        .store
        .arm(GateOperation::PutFile, "library/race/first.flac");

    let app = yevune_server::app(fixture.state.clone());
    let first_request = Request::builder()
        .uri(fixture.uri(&format!(
            "/rest/ext/moveTrack?id={id}&key=library/race/first.flac"
        )))
        .body(Body::empty())
        .unwrap();
    let first_move = tokio::spawn(async move { app.oneshot(first_request).await.unwrap() });
    entered.notified().await;
    let app = yevune_server::app(fixture.state.clone());
    let second_request = Request::builder()
        .uri(fixture.uri(&format!(
            "/rest/ext/moveTrack?id={id}&key=library/race/second.flac"
        )))
        .body(Body::empty())
        .unwrap();
    let second_move = tokio::spawn(async move { app.oneshot(second_request).await.unwrap() });
    release.notify_one();
    let first_body = json(first_move.await.unwrap()).await;
    let second_body = json(second_move.await.unwrap()).await;

    assert_eq!(
        first_body["subsonic-response"]["status"], "ok",
        "{first_body}"
    );
    assert_eq!(
        second_body["subsonic-response"]["status"], "ok",
        "{second_body}"
    );
    wait_for_track_key(&fixture.index, "library/race/second.flac").await;
    assert!(fixture
        .store
        .head("library/race/source.flac")
        .await
        .is_err());
    assert!(fixture.store.head("library/race/first.flac").await.is_err());
    assert!(fixture.store.head("library/race/second.flac").await.is_ok());
}

#[tokio::test]
async fn applied_delete_error_is_reconciled_before_index_commit() {
    let fixture = GatedFixture::new().await;
    let uploaded = json(
        fixture
            .upload("library/ambiguous/delete.flac", &flac("a.flac"))
            .await,
    )
    .await;
    let id = payload(&uploaded, "track")["id"].as_str().unwrap();
    fixture
        .store
        .fail_delete_after_apply("library/ambiguous/delete.flac");

    let deleted = json(fixture.get(&format!("/rest/ext/deleteTrack?id={id}")).await).await;
    assert_eq!(deleted["subsonic-response"]["status"], "ok", "{deleted}");
    wait_for_no_tracks(&fixture.index).await;
    assert!(fixture
        .store
        .head("library/ambiguous/delete.flac")
        .await
        .is_err());
}

#[tokio::test]
async fn applied_move_source_delete_error_keeps_committed_destination() {
    let fixture = GatedFixture::new().await;
    let uploaded = json(
        fixture
            .upload("library/ambiguous/source.flac", &flac("a.flac"))
            .await,
    )
    .await;
    let id = payload(&uploaded, "track")["id"].as_str().unwrap();
    fixture
        .store
        .fail_delete_after_apply("library/ambiguous/source.flac");

    let moved = json(
        fixture
            .get(&format!(
                "/rest/ext/moveTrack?id={id}&key=library/ambiguous/destination.flac"
            ))
            .await,
    )
    .await;
    assert_eq!(moved["subsonic-response"]["status"], "ok", "{moved}");
    wait_for_move_final(
        &fixture.index,
        &fixture.store,
        "library/ambiguous/source.flac",
        "library/ambiguous/destination.flac",
    )
    .await;
}

#[tokio::test]
async fn library_management_rejects_non_authoritative_object_keys_without_residue() {
    let fixture = Fixture::new().await;
    for key in ["inbox/a.flac", "uploads/a.flac", "library/"] {
        let rejected = json(fixture.upload(key, &flac("a.flac")).await).await;
        assert_eq!(
            rejected["subsonic-response"]["error"]["code"], 10,
            "{key}: {rejected}"
        );
        assert!(fixture.store.head(key).await.is_err(), "{key}");
    }
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tracks")
        .fetch_one(fixture.index.pool())
        .await
        .unwrap();
    assert_eq!(count, 0);

    let uploaded = json(fixture.upload("library/source.flac", &flac("a.flac")).await).await;
    let id = payload(&uploaded, "track")["id"].as_str().unwrap();
    for key in ["inbox/moved.flac", "organized/moved.flac", "library/"] {
        let rejected = json(
            fixture
                .get("admin", &format!("/rest/ext/moveTrack?id={id}&key={key}"))
                .await,
        )
        .await;
        assert_eq!(
            rejected["subsonic-response"]["error"]["code"], 10,
            "{key}: {rejected}"
        );
        assert!(fixture.store.head(key).await.is_err(), "{key}");
    }
    wait_for_track_key(&fixture.index, "library/source.flac").await;
    assert!(fixture.store.head("library/source.flac").await.is_ok());
}

#[tokio::test]
async fn failed_move_rolls_back_index_and_cleans_owned_destination() {
    let fixture = GatedFixture::new().await;
    let uploaded = json(
        fixture
            .upload("library/rollback/source.flac", &flac("a.flac"))
            .await,
    )
    .await;
    let id = payload(&uploaded, "track")["id"].as_str().unwrap();
    fixture
        .store
        .fail_delete_before_apply("library/rollback/source.flac");

    let moved = json(
        fixture
            .get(&format!(
                "/rest/ext/moveTrack?id={id}&key=library/rollback/destination.flac"
            ))
            .await,
    )
    .await;
    assert_eq!(moved["subsonic-response"]["status"], "failed", "{moved}");
    wait_for_track_key(&fixture.index, "library/rollback/source.flac").await;
    assert!(fixture
        .store
        .head("library/rollback/source.flac")
        .await
        .is_ok());
    assert!(
        fixture
            .store
            .head("library/rollback/destination.flac")
            .await
            .is_err(),
        "回滚后不得遗留 owned 目标对象"
    );
}

/// 模拟 Garage「读己之写」时序：对象 `put` 后 `head`/`get_range` 立即可见，
/// 但 `list`（LIST）尚未收敛看不到刚写入的 key。用于验证上传后的即时入库
/// 不依赖列举、对刚写入对象可靠。
struct ListLagStore {
    inner: MemoryStore,
    invisible: Mutex<HashSet<String>>,
}

impl ListLagStore {
    fn new() -> Self {
        Self {
            inner: MemoryStore::new(),
            invisible: Mutex::new(HashSet::new()),
        }
    }
}

#[async_trait]
impl ObjectStore for ListLagStore {
    async fn list(&self, prefix: &str, token: Option<String>) -> StorageResult<ListPage> {
        let page = self.inner.list(prefix, token).await?;
        let invisible = self.invisible.lock().unwrap();
        let entries = page
            .entries
            .into_iter()
            .filter(|entry| !invisible.contains(&entry.key))
            .collect();
        Ok(ListPage {
            entries,
            next_token: page.next_token,
        })
    }

    async fn get(&self, key: &str) -> StorageResult<Bytes> {
        self.inner.get(key).await
    }

    async fn get_range(&self, key: &str, range: Range<u64>) -> StorageResult<Bytes> {
        self.inner.get_range(key, range).await
    }

    async fn put(&self, key: &str, bytes: Bytes) -> StorageResult<ObjectMeta> {
        self.inner.put(key, bytes).await
    }

    async fn put_file(&self, key: &str, path: &Path) -> StorageResult<ObjectMeta> {
        let meta = self.inner.put_file(key, path).await?;
        // 写后 list 尚未可见：模拟 Garage LIST 的最终一致延迟。
        self.invisible.lock().unwrap().insert(key.to_string());
        Ok(meta)
    }

    async fn delete(&self, key: &str) -> StorageResult<()> {
        self.invisible.lock().unwrap().remove(key);
        self.inner.delete(key).await
    }

    async fn head(&self, key: &str) -> StorageResult<ObjectMeta> {
        self.inner.head(key).await
    }
}

#[tokio::test]
async fn upload_accepts_files_larger_than_default_body_limit() {
    let fixture = Fixture::new().await;
    let large = flac("large.flac");
    assert!(
        large.len() > 2 * 1024 * 1024,
        "fixture 必须大于 axum 默认 2MB body 上限，实际 {} 字节",
        large.len()
    );
    let response = fixture.upload("library/uploads/large.flac", &large).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    let id = payload(&body, "track")["id"].as_str().unwrap();
    assert!(id.starts_with("tr-"), "{body}");
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tracks WHERE object_key = 'library/uploads/large.flac'",
    )
    .fetch_one(fixture.index.pool())
    .await
    .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn upload_indexes_track_even_when_list_lags_behind_write() {
    let dir = tempfile::tempdir().unwrap();
    let index = Index::connect(&dir.path().join("yevune.sqlite"))
        .await
        .unwrap();
    let encryptor = Encryptor::new("pwd:test-secret");
    UserAdmin::new(&index, &encryptor)
        .create_user("admin", "secret", true)
        .await
        .unwrap();
    let store = Arc::new(ListLagStore::new());
    let object_store: Arc<dyn ObjectStore> = store.clone();
    let state = AppState::new(
        index.clone(),
        object_store,
        "test-secret",
        "/missing/ffmpeg",
    );

    let response = yevune_server::app(state.clone())
        .oneshot(upload_request(
            "/rest/ext/uploadTrack?u=admin&p=secret&v=1.16.1&c=test&f=json".to_string(),
            "library/uploads/lag.flac",
            &flac("a.flac"),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = json(response).await;
    let id = payload(&body, "track")["id"].as_str().unwrap();
    assert!(id.starts_with("tr-"), "{body}");

    // 前提校验：list 确实看不到刚写入的对象（否则测试无法证明不依赖列举）。
    let page = store.list("library/", None).await.unwrap();
    assert!(
        page.entries.is_empty(),
        "list 应模拟写后延迟看不到新对象，实际 {:?}",
        page.entries
    );

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tracks WHERE object_key = 'library/uploads/lag.flac'",
    )
    .fetch_one(index.pool())
    .await
    .unwrap();
    assert_eq!(count, 1, "上传后曲目应立即入库，即便列举尚未可见");
}
