//! 治理骨架端点的集成测试：`GET /healthz` 与 OpenSubsonic `GET /rest/ping`。
//!
//! 用 `tower::ServiceExt::oneshot` 直接驱动 [`Router`]，无需真实绑定端口。

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

/// 读取响应体为字符串。
async fn body_string(body: Body) -> String {
    let bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[tokio::test]
async fn healthz_返回_200() {
    let response = music_server::app()
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
async fn ping_默认返回_xml_的_ok_响应() {
    let response = music_server::app()
        .oneshot(
            Request::builder()
                .uri("/rest/ping")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let content_type = response
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert!(
        content_type.contains("xml"),
        "默认应返回 XML，实际 content-type: {content_type}"
    );

    let body = body_string(response.into_body()).await;
    assert!(
        body.contains("<subsonic-response"),
        "缺少 subsonic-response 根元素: {body}"
    );
    assert!(body.contains("status=\"ok\""), "status 应为 ok: {body}");
    assert!(body.contains("version="), "缺少 version 属性: {body}");
}

#[tokio::test]
async fn ping_支持_f_json() {
    let response = music_server::app()
        .oneshot(
            Request::builder()
                .uri("/rest/ping?f=json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let content_type = response
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert!(
        content_type.contains("json"),
        "f=json 应返回 JSON，实际 content-type: {content_type}"
    );

    let body = body_string(response.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    let resp = &json["subsonic-response"];
    assert_eq!(resp["status"], "ok");
    assert!(resp["version"].is_string(), "version 应为字符串: {body}");
}
