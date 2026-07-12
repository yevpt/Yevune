//! 管理员按对象键前缀触发增量扫描。

use axum::extract::{OriginalUri, State};
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use serde::Deserialize;

use super::super::response::{self, Format};
use super::super::{ApiAdmin, ApiQuery, AppState};

#[derive(Deserialize)]
struct ScanParams {
    prefix: Option<String>,
}

pub(super) fn router() -> Router<AppState> {
    Router::new().route("/rest/ext/startScan", get(start_scan))
}

async fn start_scan(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    ApiQuery(params): ApiQuery<ScanParams>,
    _admin: ApiAdmin,
) -> Response {
    let format = Format::from_uri(&uri);
    let Some(prefix) = params.prefix.filter(|value| !value.starts_with('/')) else {
        return response::parameter_error(format, "Required parameter 'prefix' is missing");
    };
    match state.scanner.scan(Some(&prefix)).await {
        Ok(report) => {
            let changes: Vec<_> = report
                .changes
                .iter()
                .map(|change| {
                    serde_json::json!({
                        "action": change.action.as_str(),
                        "objectKey": change.object_key,
                        "track": response::track_value(&change.track),
                    })
                })
                .collect();
            response::ok(
                format,
                serde_json::json!({"scanResult": {
                    "added": report.added,
                    "updated": report.updated,
                    "deleted": report.deleted,
                    "unchanged": report.unchanged,
                    "changes": changes,
                    "changesTruncated": report.changes_truncated
                }}),
            )
        }
        Err(crate::scanner::Error::AlreadyScanning) => {
            response::parameter_error(format, "A scan is already running")
        }
        Err(error) => {
            tracing::error!(%error, prefix = %prefix, "前缀扫描失败");
            response::internal(format)
        }
    }
}
