//! 当前用户私有的多级歌单树扩展。

use axum::extract::{OriginalUri, State};
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use serde::Deserialize;

use super::super::response::{self, Format};
use super::super::{ApiQuery, ApiUser, AppState};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateFolderParams {
    name: Option<String>,
    parent_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateFolderParams {
    id: Option<String>,
    name: Option<String>,
}

#[derive(Deserialize)]
struct IdParams {
    id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MovePlaylistParams {
    id: Option<String>,
    folder_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MoveFolderParams {
    id: Option<String>,
    parent_id: Option<String>,
}

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/rest/ext/getPlaylistTree", get(get_tree))
        .route("/rest/ext/createPlaylistFolder", get(create_folder))
        .route("/rest/ext/updatePlaylistFolder", get(update_folder))
        .route("/rest/ext/deletePlaylistFolder", get(delete_folder))
        .route("/rest/ext/movePlaylist", get(move_playlist))
        .route("/rest/ext/moveFolder", get(move_folder))
}

async fn get_tree(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    ApiUser(user): ApiUser,
) -> Response {
    let format = Format::from_uri(&uri);
    let folders = match state.index.playlists().list_folders(user.id).await {
        Ok(value) => value,
        Err(error) => {
            tracing::error!(%error, "读取歌单文件夹树失败");
            return response::internal(format);
        }
    };
    let playlists = match state.index.playlists().list_playlists(user.id).await {
        Ok(value) => value,
        Err(error) => {
            tracing::error!(%error, "读取歌单树叶子失败");
            return response::internal(format);
        }
    };
    response::ok(
        format,
        serde_json::json!({"playlistTree": {
            "folders": folders.iter().map(folder_value).collect::<Vec<_>>(),
            "playlists": playlists.iter().map(playlist_value).collect::<Vec<_>>()
        }}),
    )
}

async fn create_folder(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    ApiQuery(params): ApiQuery<CreateFolderParams>,
    ApiUser(user): ApiUser,
) -> Response {
    let format = Format::from_uri(&uri);
    let Some(name) = params.name.filter(|value| !value.trim().is_empty()) else {
        return response::parameter_error(format, "Required parameter 'name' is missing");
    };
    let parent_id = match parse_optional_folder(params.parent_id.as_deref()) {
        Ok(value) => value,
        Err(()) => return response::parameter_error(format, "parentId is malformed"),
    };
    if let Some(parent_id) = parent_id {
        match owned_folder(&state, user.id, parent_id).await {
            Ok(true) => {}
            Ok(false) => return response::not_found(format),
            Err(error) => {
                tracing::error!(%error, "检查父歌单文件夹归属失败");
                return response::internal(format);
            }
        }
    }
    let id = match state
        .index
        .playlists()
        .create_folder(user.id, name.trim(), parent_id)
        .await
    {
        Ok(value) => value,
        Err(error) => {
            tracing::error!(%error, "创建歌单文件夹失败");
            return response::internal(format);
        }
    };
    response::ok(
        format,
        serde_json::json!({"playlistFolder": {
            "id": response::opaque_id("folder", &id.to_string()),
            "ownerId": response::opaque_id("user", &user.id.to_string()),
            "name": name.trim(),
            "parentId": parent_id.map(|value| response::opaque_id("folder", &value.to_string())),
            "position": 0
        }}),
    )
}

async fn update_folder(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    ApiQuery(params): ApiQuery<UpdateFolderParams>,
    ApiUser(user): ApiUser,
) -> Response {
    let format = Format::from_uri(&uri);
    let Some(id) = params
        .id
        .as_deref()
        .and_then(|value| response::parse_entity_id(value, "folder"))
    else {
        return response::parameter_error(format, "Required parameter 'id' is missing");
    };
    let Some(name) = params.name.filter(|value| !value.trim().is_empty()) else {
        return response::parameter_error(format, "Required parameter 'name' is missing");
    };
    match owned_folder(&state, user.id, id).await {
        Ok(true) => {}
        Ok(false) => return response::not_found(format),
        Err(error) => {
            tracing::error!(%error, "检查待改歌单文件夹归属失败");
            return response::internal(format);
        }
    }
    match state.index.playlists().rename_folder(id, name.trim()).await {
        Ok(true) => response::empty(format),
        Ok(false) => response::not_found(format),
        Err(error) => {
            tracing::error!(%error, "重命名歌单文件夹失败");
            response::internal(format)
        }
    }
}

async fn delete_folder(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    ApiQuery(params): ApiQuery<IdParams>,
    ApiUser(user): ApiUser,
) -> Response {
    let format = Format::from_uri(&uri);
    let Some(id) = params
        .id
        .as_deref()
        .and_then(|value| response::parse_entity_id(value, "folder"))
    else {
        return response::parameter_error(format, "Required parameter 'id' is missing");
    };
    match owned_folder(&state, user.id, id).await {
        Ok(true) => {}
        Ok(false) => return response::not_found(format),
        Err(error) => {
            tracing::error!(%error, "检查待删歌单文件夹归属失败");
            return response::internal(format);
        }
    }
    match state.index.playlists().delete_folder(id).await {
        Ok(true) => response::empty(format),
        Ok(false) => response::not_found(format),
        Err(error) => {
            tracing::error!(%error, "删除歌单文件夹失败");
            response::internal(format)
        }
    }
}

async fn move_playlist(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    ApiQuery(params): ApiQuery<MovePlaylistParams>,
    ApiUser(user): ApiUser,
) -> Response {
    let format = Format::from_uri(&uri);
    let Some(id) = params
        .id
        .as_deref()
        .and_then(|value| response::parse_entity_id(value, "playlist"))
    else {
        return response::parameter_error(format, "Required parameter 'id' is missing");
    };
    let folder_id = match parse_optional_folder(params.folder_id.as_deref()) {
        Ok(value) => value,
        Err(()) => return response::parameter_error(format, "folderId is malformed"),
    };
    match owned_playlist(&state, user.id, id).await {
        Ok(true) => {}
        Ok(false) => return response::not_found(format),
        Err(error) => {
            tracing::error!(%error, "检查待移动歌单归属失败");
            return response::internal(format);
        }
    }
    if let Some(folder) = folder_id {
        match owned_folder(&state, user.id, folder).await {
            Ok(true) => {}
            Ok(false) => return response::not_found(format),
            Err(error) => {
                tracing::error!(%error, "检查移动目标文件夹归属失败");
                return response::internal(format);
            }
        }
    }
    match state.index.playlists().move_playlist(id, folder_id).await {
        Ok(true) => response::empty(format),
        Ok(false) => response::not_found(format),
        Err(error) => {
            tracing::error!(%error, "移动歌单失败");
            response::internal(format)
        }
    }
}

async fn move_folder(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    ApiQuery(params): ApiQuery<MoveFolderParams>,
    ApiUser(user): ApiUser,
) -> Response {
    let format = Format::from_uri(&uri);
    let Some(id) = params
        .id
        .as_deref()
        .and_then(|value| response::parse_entity_id(value, "folder"))
    else {
        return response::parameter_error(format, "Required parameter 'id' is missing");
    };
    let parent_id = match parse_optional_folder(params.parent_id.as_deref()) {
        Ok(value) => value,
        Err(()) => return response::parameter_error(format, "parentId is malformed"),
    };
    match owned_folder(&state, user.id, id).await {
        Ok(true) => {}
        Ok(false) => return response::not_found(format),
        Err(error) => {
            tracing::error!(%error, "检查待移动歌单文件夹归属失败");
            return response::internal(format);
        }
    }
    if let Some(parent) = parent_id {
        match owned_folder(&state, user.id, parent).await {
            Ok(true) => {}
            Ok(false) => return response::not_found(format),
            Err(error) => {
                tracing::error!(%error, "检查移动目标父文件夹归属失败");
                return response::internal(format);
            }
        }
    }
    let creates_cycle = match state.index.playlists().list_folders(user.id).await {
        Ok(folders) => {
            let mut cursor = parent_id.map(|value| value.to_string());
            let mut cycle = false;
            while let Some(current) = cursor {
                if current == id.to_string() {
                    cycle = true;
                    break;
                }
                cursor = folders
                    .iter()
                    .find(|folder| folder.id == current)
                    .and_then(|folder| folder.parent_id.clone());
            }
            cycle
        }
        Err(error) => {
            tracing::error!(%error, "检查歌单文件夹移动环失败");
            return response::internal(format);
        }
    };
    if creates_cycle {
        return response::parameter_error(format, "A folder cannot contain itself");
    }
    match state.index.playlists().move_folder(id, parent_id).await {
        Ok(true) => response::empty(format),
        Ok(false) => response::not_found(format),
        Err(error) => {
            tracing::error!(%error, "移动歌单文件夹失败");
            response::internal(format)
        }
    }
}

fn parse_optional_folder(value: Option<&str>) -> Result<Option<i64>, ()> {
    match value {
        Some("") | None => Ok(None),
        Some(value) => response::parse_entity_id(value, "folder")
            .map(Some)
            .ok_or(()),
    }
}

async fn owned_folder(state: &AppState, owner_id: i64, id: i64) -> sqlx::Result<bool> {
    state
        .index
        .playlists()
        .list_folders(owner_id)
        .await
        .map(|folders| folders.iter().any(|folder| folder.id == id.to_string()))
}

async fn owned_playlist(state: &AppState, owner_id: i64, id: i64) -> sqlx::Result<bool> {
    state
        .index
        .playlists()
        .get_playlist(id)
        .await
        .map(|playlist| playlist.is_some_and(|playlist| playlist.owner_id == owner_id.to_string()))
}

fn folder_value(folder: &contract::PlaylistFolder) -> serde_json::Value {
    serde_json::json!({
        "id": response::opaque_id("folder", &folder.id),
        "ownerId": response::opaque_id("user", &folder.owner_id),
        "name": folder.name,
        "parentId": folder.parent_id.as_ref().map(|id| response::opaque_id("folder", id)),
        "position": folder.position
    })
}

fn playlist_value(playlist: &contract::Playlist) -> serde_json::Value {
    serde_json::json!({
        "id": response::opaque_id("playlist", &playlist.id),
        "ownerId": response::opaque_id("user", &playlist.owner_id),
        "name": playlist.name,
        "comment": playlist.comment,
        "folderId": playlist.folder_id.as_ref().map(|id| response::opaque_id("folder", id)),
        "position": playlist.position,
        "songCount": playlist.song_count,
        "duration": playlist.duration,
        "created": playlist.created,
        "changed": playlist.changed
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::index::Index;
    use crate::storage::{MemoryStore, ObjectStore};

    use super::*;

    #[tokio::test]
    async fn ownership_helpers_propagate_database_errors() {
        let dir = tempfile::tempdir().unwrap();
        let index = Index::connect(&dir.path().join("owner.sqlite"))
            .await
            .unwrap();
        let store: Arc<dyn ObjectStore> = Arc::new(MemoryStore::new());
        let state = AppState::new(index.clone(), store, "secret", "/missing/ffmpeg");
        index.pool().close().await;

        assert!(owned_folder(&state, 1, 1).await.is_err());
        assert!(owned_playlist(&state, 1, 1).await.is_err());
    }
}
