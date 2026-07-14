//! 系统类端点：健康、登录探测、许可与扩展发现。

use axum::extract::OriginalUri;
use axum::http::StatusCode;
use axum::response::Response;
use axum::routing::get;
use axum::Router;

use super::response::{self, Format};
use super::{ApiUser, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/rest/ping", get(ping))
        .route("/rest/ping.view", get(ping))
        .route("/rest/getLicense", get(get_license))
        .route("/rest/getLicense.view", get(get_license))
        .route(
            "/rest/getOpenSubsonicExtensions",
            get(get_open_subsonic_extensions),
        )
        .route(
            "/rest/getOpenSubsonicExtensions.view",
            get(get_open_subsonic_extensions),
        )
}

async fn healthz() -> StatusCode {
    StatusCode::OK
}

async fn ping(OriginalUri(uri): OriginalUri, _user: ApiUser) -> Response {
    response::empty(Format::from_uri(&uri))
}

async fn get_license(OriginalUri(uri): OriginalUri, _user: ApiUser) -> Response {
    response::ok(
        Format::from_uri(&uri),
        serde_json::json!({"license": {"valid": true}}),
    )
}

async fn get_open_subsonic_extensions(OriginalUri(uri): OriginalUri) -> Response {
    response::ok(
        Format::from_uri(&uri),
        serde_json::json!({"openSubsonicExtensions": [
            {"name": "playlistTree", "versions": [1]},
            {"name": "libraryManagement", "versions": [1]},
            {"name": "accessControl", "versions": [1]},
            {"name": "roleManagement", "versions": [1]},
            {"name": "userManagement", "versions": [1]},
            {"name": "prefixScan", "versions": [1]}
            ,{"name": "coverArtManagement", "versions": [1]}
        ]}),
    )
}
