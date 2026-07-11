//! 浏览类端点：`getArtists`/`getIndexes`/`getArtist`/`getAlbum`/`getSong`/`getAlbumList2`。
//!
//! 每个端点解析当前用户为 [`Viewer`]，改走 MediaRepo 的 `*_visible` 读方法，把曲库访问控制
//! 统一强制在数据层（设计文档 §6）。受限内容对无授权者一律以「未找到」(70) 遮蔽，避免存在性泄漏。

use std::collections::HashMap;

use axum::extract::{Query, State};
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use serde_json::{Map, Value};

use super::response::{self, Format, ERROR_MISSING_PARAM, ERROR_NOT_FOUND};
use super::state::AppState;
use crate::auth::CurrentUser;
use crate::index::Viewer;

/// 浏览端点路由。
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/rest/getArtists", get(get_artists))
        .route("/rest/getIndexes", get(get_indexes))
        .route("/rest/getArtist", get(get_artist))
        .route("/rest/getAlbum", get(get_album))
        .route("/rest/getSong", get(get_song))
        .route("/rest/getAlbumList2", get(get_album_list2))
}

/// 解析查询参数 `f` 的响应格式。
fn format_of(params: &HashMap<String, String>) -> Format {
    Format::from_param(params.get("f").map(String::as_str))
}

/// 解析整型 id 参数。
fn param_i64(params: &HashMap<String, String>, key: &str) -> Option<i64> {
    params.get(key).and_then(|v| v.parse().ok())
}

/// 解析当前用户为 [`Viewer`]（服务端权威角色/管理员判定）。
async fn viewer_of(state: &AppState, user: &CurrentUser) -> Result<Viewer, sqlx::Error> {
    state.index.access_control().resolve_viewer(user.id).await
}

/// 把 DTO 序列化为 JSON 对象（供并入响应载荷）。
fn to_object<T: serde::Serialize>(value: &T) -> Map<String, Value> {
    match serde_json::to_value(value) {
        Ok(Value::Object(m)) => m,
        _ => Map::new(),
    }
}

/// `GET /rest/getSong` —— 取单曲目，受限则对无授权者遮蔽。
async fn get_song(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let format = format_of(&params);
    let Some(id) = param_i64(&params, "id") else {
        return response::error(format, ERROR_MISSING_PARAM, "缺少参数 id");
    };
    let Ok(viewer) = viewer_of(&state, &user).await else {
        return response::error(format, 0, "内部错误");
    };
    match state.index.media().get_track_visible(&viewer, id).await {
        Ok(Some(track)) => {
            let mut payload = Map::new();
            payload.insert("song".into(), Value::Object(to_object(&track)));
            response::ok(format, Value::Object(payload))
        }
        Ok(None) => response::error(format, ERROR_NOT_FOUND, "曲目不存在"),
        Err(_) => response::error(format, 0, "内部错误"),
    }
}

/// `GET /rest/getAlbum` —— 取专辑元数据 + 其**可见**曲目列表。
async fn get_album(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let format = format_of(&params);
    let Some(id) = param_i64(&params, "id") else {
        return response::error(format, ERROR_MISSING_PARAM, "缺少参数 id");
    };
    let Ok(viewer) = viewer_of(&state, &user).await else {
        return response::error(format, 0, "内部错误");
    };
    let media = state.index.media();
    match media.get_album_visible(&viewer, id).await {
        Ok(Some(album)) => {
            let songs = match media.album_tracks_visible(&viewer, id).await {
                Ok(s) => s,
                Err(_) => return response::error(format, 0, "内部错误"),
            };
            let mut album_obj = to_object(&album);
            let song_values: Vec<Value> =
                songs.iter().map(|t| Value::Object(to_object(t))).collect();
            album_obj.insert("song".into(), Value::Array(song_values));
            let mut payload = Map::new();
            payload.insert("album".into(), Value::Object(album_obj));
            response::ok(format, Value::Object(payload))
        }
        Ok(None) => response::error(format, ERROR_NOT_FOUND, "专辑不存在"),
        Err(_) => response::error(format, 0, "内部错误"),
    }
}

/// `GET /rest/getArtist` —— 取艺人 + 其**可见**专辑列表。
async fn get_artist(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let format = format_of(&params);
    let Some(id) = param_i64(&params, "id") else {
        return response::error(format, ERROR_MISSING_PARAM, "缺少参数 id");
    };
    let Ok(viewer) = viewer_of(&state, &user).await else {
        return response::error(format, 0, "内部错误");
    };
    let media = state.index.media();
    match media.get_artist_visible(&viewer, id).await {
        Ok(Some(artist)) => {
            let albums = match media.artist_albums_visible(&viewer, id).await {
                Ok(a) => a,
                Err(_) => return response::error(format, 0, "内部错误"),
            };
            let mut artist_obj = to_object(&artist);
            let album_values: Vec<Value> =
                albums.iter().map(|a| Value::Object(to_object(a))).collect();
            artist_obj.insert("album".into(), Value::Array(album_values));
            let mut payload = Map::new();
            payload.insert("artist".into(), Value::Object(artist_obj));
            response::ok(format, Value::Object(payload))
        }
        Ok(None) => response::error(format, ERROR_NOT_FOUND, "艺人不存在"),
        Err(_) => response::error(format, 0, "内部错误"),
    }
}

/// `GET /rest/getArtists` —— 按首字母分组列出**可见**艺人。
async fn get_artists(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    artists_grouped(&state, &user, &params, "artists").await
}

/// `GET /rest/getIndexes` —— 结构同 `getArtists`，根键为 `indexes`。
async fn get_indexes(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    artists_grouped(&state, &user, &params, "indexes").await
}

/// 公共实现：把可见艺人按首字母分组为 `<index>`。
async fn artists_grouped(
    state: &AppState,
    user: &CurrentUser,
    params: &HashMap<String, String>,
    root_key: &str,
) -> Response {
    let format = format_of(params);
    let Ok(viewer) = viewer_of(state, user).await else {
        return response::error(format, 0, "内部错误");
    };
    let artists = match state.index.media().list_artists_visible(&viewer).await {
        Ok(a) => a,
        Err(_) => return response::error(format, 0, "内部错误"),
    };

    // 按首字母（sort_name 优先）分组，保持组内原有顺序。
    let mut groups: Vec<(String, Vec<Value>)> = Vec::new();
    for artist in &artists {
        let key = index_key(artist.sort_name.as_deref().unwrap_or(&artist.name));
        let entry = match groups.iter_mut().find(|(k, _)| *k == key) {
            Some(e) => e,
            None => {
                groups.push((key.clone(), Vec::new()));
                groups.last_mut().unwrap()
            }
        };
        entry.1.push(Value::Object(to_object(artist)));
    }

    let index_values: Vec<Value> = groups
        .into_iter()
        .map(|(name, artist_values)| {
            let mut obj = Map::new();
            obj.insert("name".into(), Value::from(name));
            obj.insert("artist".into(), Value::Array(artist_values));
            Value::Object(obj)
        })
        .collect();

    let mut inner = Map::new();
    inner.insert("ignoredArticles".into(), Value::from(""));
    inner.insert("index".into(), Value::Array(index_values));
    let mut payload = Map::new();
    payload.insert(root_key.into(), Value::Object(inner));
    response::ok(format, Value::Object(payload))
}

/// 取分组首字母：首个字符大写，非字母归入 `#`。
fn index_key(name: &str) -> String {
    match name.chars().next() {
        Some(c) if c.is_alphabetic() => c.to_uppercase().to_string(),
        Some(_) => "#".to_string(),
        None => "#".to_string(),
    }
}

/// `GET /rest/getAlbumList2` —— 列出**可见**专辑，支持 `size`/`offset` 分页。
async fn get_album_list2(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let format = format_of(&params);
    let Ok(viewer) = viewer_of(&state, &user).await else {
        return response::error(format, 0, "内部错误");
    };
    let mut albums = match state.index.media().list_albums_visible(&viewer).await {
        Ok(a) => a,
        Err(_) => return response::error(format, 0, "内部错误"),
    };

    // 分页（默认 size=10，对齐 OpenSubsonic 默认；offset 默认 0）。
    let offset = param_i64(&params, "offset").unwrap_or(0).max(0) as usize;
    let size = param_i64(&params, "size").unwrap_or(10).clamp(0, 500) as usize;
    if offset < albums.len() {
        albums = albums[offset..].to_vec();
    } else {
        albums.clear();
    }
    albums.truncate(size);

    let album_values: Vec<Value> = albums.iter().map(|a| Value::Object(to_object(a))).collect();
    let mut inner = Map::new();
    inner.insert("album".into(), Value::Array(album_values));
    let mut payload = Map::new();
    payload.insert("albumList2".into(), Value::Object(inner));
    response::ok(format, Value::Object(payload))
}
