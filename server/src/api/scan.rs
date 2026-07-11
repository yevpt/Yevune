//! 曲库扫描状态与管理员触发端点。

use axum::extract::{OriginalUri, State};
use axum::response::Response;
use axum::routing::get;
use axum::Router;

use super::response::{self, Format};
use super::{ApiAdmin, ApiUser, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/rest/getScanStatus", get(get_scan_status))
        .route("/rest/getScanStatus.view", get(get_scan_status))
        .route("/rest/startScan", get(start_scan))
        .route("/rest/startScan.view", get(start_scan))
}

async fn get_scan_status(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    _user: ApiUser,
) -> Response {
    scan_status_response(Format::from_uri(&uri), state.scanner.scan_status())
}

async fn start_scan(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    ApiAdmin(_admin): ApiAdmin,
) -> Response {
    let format = Format::from_uri(&uri);
    state.scanner.try_start(None);
    scan_status_response(format, state.scanner.scan_status())
}

fn scan_status_response(format: Format, status: crate::scanner::ScanStatus) -> Response {
    response::ok(
        format,
        serde_json::json!({
            "scanStatus": {
                "scanning": status.scanning,
                "count": status.scanned,
            }
        }),
    )
}
