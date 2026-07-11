//! 媒体端点：`stream`/`download`/`getCoverArt`，均先做曲库访问控制门控（设计文档 §6）。
//!
//! 授权后走 [`ObjectStore`] **有界分块**流式透传原始对象（红线：绝不把整个音频文件读进内存）；
//! 无授权者一律以 subsonic 错误 70 遮蔽，避免存在性泄漏。转码（T5）暂不接入，先直传原格式。

use std::collections::HashMap;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use bytes::Bytes;
use futures::stream;

use super::response::{self, Format, ERROR_MISSING_PARAM, ERROR_NOT_FOUND};
use super::state::AppState;
use crate::auth::CurrentUser;
use crate::index::Viewer;
use crate::storage::ObjectStore;

/// 流式透传的分块大小（有界缓冲）。
const CHUNK: u64 = 64 * 1024;

/// 媒体端点路由。
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/rest/stream", get(stream))
        .route("/rest/download", get(download))
        .route("/rest/getCoverArt", get(get_cover_art))
}

fn format_of(params: &HashMap<String, String>) -> Format {
    Format::from_param(params.get("f").map(String::as_str))
}

fn param_i64(params: &HashMap<String, String>, key: &str) -> Option<i64> {
    params.get(key).and_then(|v| v.parse().ok())
}

async fn viewer_of(state: &AppState, user: &CurrentUser) -> Result<Viewer, sqlx::Error> {
    state.index.access_control().resolve_viewer(user.id).await
}

/// `GET /rest/stream` —— 门控后透传原始音频（暂不转码）。
async fn stream(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    deliver(state, user, params, false).await
}

/// `GET /rest/download` —— 门控后透传原始音频，附下载头。
async fn download(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    deliver(state, user, params, true).await
}

/// stream/download 公共实现：先门控 `can_access_track`，再有界流式透传。
async fn deliver(
    state: AppState,
    user: CurrentUser,
    params: HashMap<String, String>,
    as_attachment: bool,
) -> Response {
    let format = format_of(&params);
    let Some(id) = param_i64(&params, "id") else {
        return response::error(format, ERROR_MISSING_PARAM, "缺少参数 id");
    };
    let Ok(viewer) = viewer_of(&state, &user).await else {
        return response::error(format, 0, "内部错误");
    };
    // 访问控制门控：不可见一律以「未找到」遮蔽（不区分不存在/无权限）。
    match state
        .index
        .access_control()
        .can_access_track(&viewer, id)
        .await
    {
        Ok(true) => {}
        Ok(false) => return response::error(format, ERROR_NOT_FOUND, "曲目不存在"),
        Err(_) => return response::error(format, 0, "内部错误"),
    }
    let (object_key, codec) = match state.index.media().track_source(id).await {
        Ok(Some(src)) => src,
        Ok(None) => return response::error(format, ERROR_NOT_FOUND, "曲目不存在"),
        Err(_) => return response::error(format, 0, "内部错误"),
    };
    let size = match state.store.head(&object_key).await {
        Ok(meta) => meta.size,
        Err(_) => return response::error(format, ERROR_NOT_FOUND, "对象不存在"),
    };

    let body = object_body(state.store.clone(), object_key, size);
    let mut builder = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime_for(codec.as_deref()))
        .header(header::CONTENT_LENGTH, size)
        .header(header::ACCEPT_RANGES, "bytes");
    if as_attachment {
        let ext = codec.as_deref().unwrap_or("bin");
        builder = builder.header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{id}.{ext}\""),
        );
    }
    builder.body(body).expect("构建响应")
}

/// `GET /rest/getCoverArt` —— 按封面归属的可见性门控后返回封面字节。
async fn get_cover_art(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let format = format_of(&params);
    let Some(cover_key) = params.get("id") else {
        return response::error(format, ERROR_MISSING_PARAM, "缺少参数 id");
    };
    let Ok(viewer) = viewer_of(&state, &user).await else {
        return response::error(format, 0, "内部错误");
    };
    match state
        .index
        .media()
        .cover_key_visible(&viewer, cover_key)
        .await
    {
        Ok(true) => {}
        Ok(false) => return response::error(format, ERROR_NOT_FOUND, "封面不存在"),
        Err(_) => return response::error(format, 0, "内部错误"),
    }
    // 封面为小图，直接读取（红线约束的是整段音频，不含封面缩略图）。
    match state.store.get(cover_key).await {
        Ok(bytes) => ([(header::CONTENT_TYPE, cover_mime(cover_key))], bytes).into_response(),
        Err(_) => response::error(format, ERROR_NOT_FOUND, "封面不存在"),
    }
}

/// 以 `get_range` 分块拉取 `[0, size)`，构建有界内存的流式响应体。
fn object_body(store: Arc<dyn ObjectStore>, key: String, size: u64) -> Body {
    let s = stream::unfold(0u64, move |pos| {
        let store = store.clone();
        let key = key.clone();
        async move {
            if pos >= size {
                return None;
            }
            let end = (pos + CHUNK).min(size);
            match store.get_range(&key, pos..end).await {
                Ok(bytes) if !bytes.is_empty() => {
                    let next = pos + bytes.len() as u64;
                    Some((Ok::<Bytes, std::io::Error>(bytes), next))
                }
                Ok(_) => None,
                Err(e) => Some((Err(std::io::Error::other(e.to_string())), size)),
            }
        }
    });
    Body::from_stream(s)
}

/// 由编码猜测音频 MIME 类型。
fn mime_for(codec: Option<&str>) -> &'static str {
    match codec.unwrap_or("").to_ascii_lowercase().as_str() {
        "flac" => "audio/flac",
        "mp3" => "audio/mpeg",
        "m4a" | "aac" | "alac" => "audio/mp4",
        "ogg" | "opus" => "audio/ogg",
        "wav" => "audio/wav",
        _ => "application/octet-stream",
    }
}

/// 由封面键后缀猜测图片 MIME 类型。
fn cover_mime(key: &str) -> &'static str {
    let lower = key.to_ascii_lowercase();
    if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else {
        "image/jpeg"
    }
}
