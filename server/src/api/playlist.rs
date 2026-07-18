//! 当前用户私有歌单的 OpenSubsonic 扁平兼容视图。

use axum::extract::{OriginalUri, State};
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use serde::Deserialize;

use super::response::{self, Format};
use super::{ApiQuery, ApiUser, AppState};

#[derive(Deserialize)]
struct IdParams {
    id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateParams {
    name: Option<String>,
    playlist_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateParams {
    playlist_id: Option<String>,
    name: Option<String>,
    comment: Option<String>,
}

pub fn router() -> Router<AppState> {
    let mut router = Router::new();
    for path in ["/rest/getPlaylists", "/rest/getPlaylists.view"] {
        router = router.route(path, get(get_playlists));
    }
    for path in ["/rest/getPlaylist", "/rest/getPlaylist.view"] {
        router = router.route(path, get(get_playlist));
    }
    for path in ["/rest/createPlaylist", "/rest/createPlaylist.view"] {
        router = router.route(path, get(create_playlist));
    }
    for path in ["/rest/updatePlaylist", "/rest/updatePlaylist.view"] {
        router = router.route(path, get(update_playlist));
    }
    for path in ["/rest/deletePlaylist", "/rest/deletePlaylist.view"] {
        router = router.route(path, get(delete_playlist));
    }
    router
}

async fn get_playlists(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    ApiUser(user): ApiUser,
) -> Response {
    let format = Format::from_uri(&uri);
    match state.index.playlists().list_playlists(user.id).await {
        Ok(playlists) => {
            let values: Vec<_> = playlists
                .iter()
                .map(|playlist| response::playlist_value(playlist, &user.name))
                .collect();
            response::ok(
                format,
                serde_json::json!({"playlists": {"playlist": values}}),
            )
        }
        Err(error) => {
            tracing::error!(%error, "getPlaylists 查询失败");
            response::internal(format)
        }
    }
}

async fn get_playlist(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    ApiQuery(params): ApiQuery<IdParams>,
    ApiUser(user): ApiUser,
) -> Response {
    let format = Format::from_uri(&uri);
    let Some(id) = params
        .id
        .as_deref()
        .and_then(|id| response::parse_entity_id(id, "playlist"))
    else {
        return response::parameter_error(format, "Required parameter 'id' is missing");
    };
    playlist_detail(&state, format, &user, id).await
}

async fn create_playlist(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    ApiQuery(params): ApiQuery<CreateParams>,
    ApiUser(user): ApiUser,
) -> Response {
    let format = Format::from_uri(&uri);
    let song_ids = match response::query_entity_ids(&uri, "songId", "track") {
        Ok(ids) => ids,
        Err(()) => return response::parameter_error(format, "songId is malformed"),
    };
    if let Some(raw_id) = params.playlist_id.as_deref() {
        let Some(id) = response::parse_entity_id(raw_id, "playlist") else {
            return response::parameter_error(format, "playlistId is malformed");
        };
        if owned_playlist(&state, user.id, id, format).await.is_none() {
            return response::not_found(format);
        }
        if let Err(error) = state.index.playlists().set_tracks(id, &song_ids).await {
            tracing::error!(%error, "createPlaylist 替换歌曲失败");
            return response::internal(format);
        }
        return playlist_detail(&state, format, &user, id).await;
    }
    let Some(name) = params
        .name
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
    else {
        return response::parameter_error(format, "Required parameter 'name' is missing");
    };
    let id = match state
        .index
        .playlists()
        .create_playlist_with_tracks(user.id, &name, None, &song_ids)
        .await
    {
        Ok(id) => id,
        Err(error) => {
            tracing::error!(%error, "createPlaylist 创建失败");
            return response::internal(format);
        }
    };
    playlist_detail(&state, format, &user, id).await
}

async fn update_playlist(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    ApiQuery(params): ApiQuery<UpdateParams>,
    ApiUser(user): ApiUser,
) -> Response {
    let format = Format::from_uri(&uri);
    let Some(id) = params
        .playlist_id
        .as_deref()
        .and_then(|id| response::parse_entity_id(id, "playlist"))
    else {
        return response::parameter_error(format, "Required parameter 'playlistId' is missing");
    };
    let add_ids = match response::query_entity_ids(&uri, "songIdToAdd", "track") {
        Ok(ids) => ids,
        Err(()) => return response::parameter_error(format, "songIdToAdd is malformed"),
    };
    let mut remove_indices = match response::query_i64_values(&uri, "songIndexToRemove") {
        Ok(indices) => indices,
        Err(()) => return response::parameter_error(format, "songIndexToRemove is malformed"),
    };
    let Some(playlist) = owned_playlist(&state, user.id, id, format).await else {
        return response::not_found(format);
    };
    let submitted_name = params.name.as_deref().map(str::trim);
    if submitted_name == Some("") {
        return response::parameter_error(format, "Playlist name must not be blank");
    }
    let name = submitted_name.unwrap_or(&playlist.name);
    let comment = params.comment.as_deref().or(playlist.comment.as_deref());
    if !add_ids.is_empty() || !remove_indices.is_empty() {
        let mut tracks = match state.index.playlists().track_ids(id).await {
            Ok(tracks) => tracks,
            Err(error) => {
                tracing::error!(%error, "updatePlaylist 曲目读取失败");
                return response::internal(format);
            }
        };
        remove_indices.sort_unstable_by(|a, b| b.cmp(a));
        for index in remove_indices {
            if index >= 0 && (index as usize) < tracks.len() {
                tracks.remove(index as usize);
            }
        }
        tracks.extend(add_ids);
        if let Err(error) = state
            .index
            .playlists()
            .update_playlist_with_tracks(id, name, comment, &tracks)
            .await
        {
            tracing::error!(%error, "updatePlaylist 原子更新失败");
            return response::internal(format);
        }
    } else if let Err(error) = state
        .index
        .playlists()
        .update_playlist(id, name, comment)
        .await
    {
        tracing::error!(%error, "updatePlaylist 元数据更新失败");
        return response::internal(format);
    }
    response::empty(format)
}

async fn delete_playlist(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    ApiQuery(params): ApiQuery<IdParams>,
    ApiUser(user): ApiUser,
) -> Response {
    let format = Format::from_uri(&uri);
    let Some(id) = params
        .id
        .as_deref()
        .and_then(|id| response::parse_entity_id(id, "playlist"))
    else {
        return response::parameter_error(format, "Required parameter 'id' is missing");
    };
    if owned_playlist(&state, user.id, id, format).await.is_none() {
        return response::not_found(format);
    }
    match state.index.playlists().delete_playlist(id).await {
        Ok(true) => response::empty(format),
        Ok(false) => response::not_found(format),
        Err(error) => {
            tracing::error!(%error, "deletePlaylist 删除失败");
            response::internal(format)
        }
    }
}

async fn owned_playlist(
    state: &AppState,
    owner_id: i64,
    id: i64,
    _format: Format,
) -> Option<contract::Playlist> {
    match state.index.playlists().get_playlist(id).await {
        Ok(Some(playlist)) if playlist.owner_id == owner_id.to_string() => Some(playlist),
        Ok(_) => None,
        Err(error) => {
            tracing::error!(%error, "歌单所有权查询失败");
            None
        }
    }
}

async fn playlist_detail(
    state: &AppState,
    format: Format,
    user: &crate::auth::CurrentUser,
    id: i64,
) -> Response {
    let Some(playlist) = owned_playlist(state, user.id, id, format).await else {
        return response::not_found(format);
    };
    let ids = match state.index.playlists().track_ids(id).await {
        Ok(ids) => ids,
        Err(error) => {
            tracing::error!(%error, "getPlaylist 曲目主键查询失败");
            return response::internal(format);
        }
    };
    // 访问控制强制：歌单展开时逐条按可见性过滤，受限曲目对无授权用户不出现在条目里。
    let viewer = match state.viewer(user.id).await {
        Ok(viewer) => viewer,
        Err(error) => {
            tracing::error!(%error, "getPlaylist 解析访问者失败");
            return response::internal(format);
        }
    };
    let mut entries = Vec::with_capacity(ids.len());
    for track_id in ids {
        match state
            .index
            .media()
            .get_track_visible(&viewer, track_id)
            .await
        {
            Ok(Some(track)) => entries.push(response::track_value(&track)),
            Ok(None) => {}
            Err(error) => {
                tracing::error!(%error, "getPlaylist 曲目查询失败");
                return response::internal(format);
            }
        }
    }
    let mut value = response::playlist_value(&playlist, &user.name);
    value
        .as_object_mut()
        .expect("playlist 是对象")
        .insert("entry".into(), entries.into());
    response::ok(format, serde_json::json!({"playlist": value}))
}
