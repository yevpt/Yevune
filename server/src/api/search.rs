//! 搜索端点：`search3`（走 FTS5），结果按当前用户曲库访问控制过滤。

use std::collections::HashMap;

use axum::extract::{Query, State};
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use serde_json::{Map, Value};

use super::response::{self, Format};
use super::state::AppState;
use crate::auth::CurrentUser;

/// 搜索端点路由。
pub fn router() -> Router<AppState> {
    Router::new().route("/rest/search3", get(search3))
}

/// `GET /rest/search3` —— FTS5 搜索，仅返回当前用户可见的命中。
async fn search3(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let format = Format::from_param(params.get("f").map(String::as_str));
    let query = params.get("query").map(String::as_str).unwrap_or("").trim();

    let mut result = Map::new();
    // 空查询直接返回空结果集；trigram FTS 对过短查询无匹配。
    if !query.is_empty() {
        let Ok(viewer) = state.index.access_control().resolve_viewer(user.id).await else {
            return response::error(format, 0, "内部错误");
        };
        // 取各类型上限（对齐 OpenSubsonic 默认 20），统一用较大 limit 拉取后再分组。
        let limit = params
            .get("songCount")
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(20)
            .clamp(1, 500);
        match state
            .index
            .media()
            .search_visible(&viewer, query, limit)
            .await
        {
            Ok(hits) => {
                result.insert(
                    "artist".into(),
                    Value::Array(hits.artists.iter().map(to_value).collect()),
                );
                result.insert(
                    "album".into(),
                    Value::Array(hits.albums.iter().map(to_value).collect()),
                );
                result.insert(
                    "song".into(),
                    Value::Array(hits.tracks.iter().map(to_value).collect()),
                );
            }
            Err(_) => return response::error(format, 0, "内部错误"),
        }
    }

    let mut payload = Map::new();
    payload.insert("searchResult3".into(), Value::Object(result));
    response::ok(format, Value::Object(payload))
}

/// 序列化 DTO 为 JSON 值。
fn to_value<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}
