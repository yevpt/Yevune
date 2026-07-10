//! 系统类端点：健康检查与 OpenSubsonic `ping`。

use axum::extract::Query;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use serde::Deserialize;

/// 声明兼容的 OpenSubsonic/Subsonic API 协议版本。
const API_VERSION: &str = "1.16.1";
/// 服务端标识（对应 subsonic-response 的 `type` 字段）。
const SERVER_TYPE: &str = "music-server";

/// 本模块的路由。
pub fn router() -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/rest/ping", get(ping))
}

/// `GET /healthz` —— 轻量存活探针，恒返回 200。
async fn healthz() -> StatusCode {
    StatusCode::OK
}

/// OpenSubsonic 请求的通用查询参数（此处仅关心响应格式 `f`）。
#[derive(Debug, Deserialize)]
struct SubsonicParams {
    /// 响应格式：`json` 返回 JSON，其余（含缺省）返回 XML。
    f: Option<String>,
}

/// `GET /rest/ping` —— 返回标准 subsonic-response ok 信封，支持 `f=json`。
async fn ping(Query(params): Query<SubsonicParams>) -> Response {
    if params.f.as_deref() == Some("json") {
        ok_json().into_response()
    } else {
        ok_xml().into_response()
    }
}

/// 构建 XML 格式的 ok 响应。
fn ok_xml() -> impl IntoResponse {
    let body = format!(
        concat!(
            r#"<?xml version="1.0" encoding="UTF-8"?>"#,
            "\n",
            r#"<subsonic-response xmlns="http://subsonic.org/restapi" status="ok" version="{version}" type="{type}" serverVersion="{server_version}" openSubsonic="true"/>"#,
        ),
        version = API_VERSION,
        r#type = SERVER_TYPE,
        server_version = env!("CARGO_PKG_VERSION"),
    );
    (
        [(header::CONTENT_TYPE, "application/xml; charset=utf-8")],
        body,
    )
}

/// 构建 JSON 格式的 ok 响应。
fn ok_json() -> impl IntoResponse {
    let body = serde_json::json!({
        "subsonic-response": {
            "status": "ok",
            "version": API_VERSION,
            "type": SERVER_TYPE,
            "serverVersion": env!("CARGO_PKG_VERSION"),
            "openSubsonic": true,
        }
    });
    axum::Json(body)
}
