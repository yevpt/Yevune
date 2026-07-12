//! 健康检查与公共扩展发现端点测试。

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;
use yevune_server::api::AppState;
use yevune_server::index::Index;
use yevune_server::storage::{MemoryStore, ObjectStore};

async fn test_app() -> axum::Router {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.keep().join("health.sqlite");
    let index = Index::connect(&path).await.unwrap();
    let store: Arc<dyn ObjectStore> = Arc::new(MemoryStore::new());
    yevune_server::app(AppState::new(index, store, "test", "/missing/ffmpeg"))
}

#[tokio::test]
async fn healthz_returns_200() {
    let response = test_app()
        .await
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn extensions_supports_public_json_discovery() {
    let response = test_app()
        .await
        .oneshot(
            Request::builder()
                .uri("/rest/getOpenSubsonicExtensions?f=json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers()[axum::http::header::CONTENT_TYPE]
        .to_str()
        .unwrap()
        .contains("json"));
}
