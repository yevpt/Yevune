//! 管理员曲库写操作扩展。

mod operation;

use axum::extract::{Multipart, OriginalUri, State};
use axum::response::Response;
use axum::routing::{get, post};
use axum::Router;
use serde::Deserialize;
use tempfile::NamedTempFile;
use tokio::io::AsyncWriteExt;

use super::super::response::{self, Format};
use super::super::{ApiAdmin, ApiQuery, AppState};
use operation::{commit_delete, commit_move, commit_upload, commit_write_back, OperationError};

#[derive(Deserialize)]
struct IdParams {
    id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TagParams {
    id: Option<String>,
    title: Option<String>,
    album: Option<String>,
    artist: Option<String>,
    genre: Option<String>,
    year: Option<String>,
    track: Option<String>,
    disc_number: Option<String>,
}

#[derive(Deserialize)]
struct MoveParams {
    id: Option<String>,
    key: Option<String>,
}

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/rest/ext/uploadTrack", post(upload_track))
        .route("/rest/ext/updateTags", get(update_tags))
        .route("/rest/ext/writeBackTags", get(write_back_tags))
        .route("/rest/ext/deleteTrack", get(delete_track))
        .route("/rest/ext/moveTrack", get(move_track))
}

async fn upload_track(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    _admin: ApiAdmin,
    mut multipart: Multipart,
) -> Response {
    let format = Format::from_uri(&uri);
    let mut key = None;
    let mut temp = None;
    loop {
        let field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(error) => {
                tracing::warn!(%error, "解析上传 multipart 失败");
                return response::parameter_error(format, "Malformed multipart body");
            }
        };
        match field.name() {
            Some("key") => match field.text().await {
                Ok(value) if valid_key(&value) => key = Some(value),
                _ => return response::parameter_error(format, "key is malformed"),
            },
            Some("file") => {
                let file = match NamedTempFile::new() {
                    Ok(value) => value,
                    Err(error) => {
                        tracing::error!(%error, "创建上传临时文件失败");
                        return response::internal(format);
                    }
                };
                let mut output = match tokio::fs::File::create(file.path()).await {
                    Ok(value) => value,
                    Err(error) => {
                        tracing::error!(%error, "打开上传临时文件失败");
                        return response::internal(format);
                    }
                };
                let mut field = field;
                loop {
                    match field.chunk().await {
                        Ok(Some(chunk)) => {
                            if let Err(error) = output.write_all(&chunk).await {
                                tracing::error!(%error, "流式写上传临时文件失败");
                                return response::internal(format);
                            }
                        }
                        Ok(None) => break,
                        Err(error) => {
                            tracing::warn!(%error, "读取上传分块失败");
                            return response::parameter_error(format, "Malformed multipart body");
                        }
                    }
                }
                if let Err(error) = output.flush().await {
                    tracing::error!(%error, "刷新上传临时文件失败");
                    return response::internal(format);
                }
                temp = Some(file);
            }
            _ => {}
        }
    }
    let (Some(key), Some(temp)) = (key, temp) else {
        return response::parameter_error(format, "key and file are required");
    };
    let owned_state = state.clone();
    let task = tokio::spawn(async move { commit_upload(owned_state, key, temp).await });
    let id = match task.await {
        Ok(Ok(id)) => id,
        Ok(Err(_)) => return response::internal(format),
        Err(error) => {
            tracing::error!(%error, "上传 owned operation 异常终止");
            return response::internal(format);
        }
    };
    match state.index.media().get_track(id).await {
        Ok(Some(track)) => response::ok(
            format,
            serde_json::json!({"track": response::track_value(&track)}),
        ),
        Ok(None) => response::internal(format),
        Err(error) => {
            tracing::error!(%error, "读取上传曲目 DTO 失败");
            response::internal(format)
        }
    }
}

async fn update_tags(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    ApiQuery(params): ApiQuery<TagParams>,
    _admin: ApiAdmin,
) -> Response {
    let format = Format::from_uri(&uri);
    let Some(id) = parse_track(params.id.as_deref()) else {
        return response::parameter_error(format, "Required parameter 'id' is missing");
    };
    if !valid_tag_numbers(&params) {
        return response::parameter_error(format, "Numeric tag is malformed or out of range");
    }
    let values = tag_values(&params);
    if values.is_empty() {
        return response::parameter_error(format, "At least one tag is required");
    }
    match state.index.media().set_tag_overrides(id, &values).await {
        Ok(true) => response::empty(format),
        Ok(false) => response::not_found(format),
        Err(error) => {
            tracing::error!(%error, "写标签覆盖层失败");
            response::internal(format)
        }
    }
}

async fn write_back_tags(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    ApiQuery(params): ApiQuery<TagParams>,
    _admin: ApiAdmin,
) -> Response {
    let format = Format::from_uri(&uri);
    let Some(id) = parse_track(params.id.as_deref()) else {
        return response::parameter_error(format, "Required parameter 'id' is missing");
    };
    if !valid_tag_numbers(&params) {
        return response::parameter_error(format, "Numeric tag is malformed or out of range");
    }
    if tag_values(&params).is_empty() {
        return response::parameter_error(format, "At least one tag is required");
    }
    let task = tokio::spawn(commit_write_back(state, id, params));
    match task.await {
        Ok(Ok(())) => response::empty(format),
        Ok(Err(OperationError::NotFound)) => response::not_found(format),
        Ok(Err(OperationError::InvalidTags)) => {
            response::parameter_error(format, "Tags cannot be written to this file")
        }
        Ok(Err(OperationError::Internal | OperationError::DestinationExists)) => {
            response::internal(format)
        }
        Err(error) => {
            tracing::error!(%error, "写回 owned operation 异常终止");
            response::internal(format)
        }
    }
}

async fn delete_track(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    ApiQuery(params): ApiQuery<IdParams>,
    _admin: ApiAdmin,
) -> Response {
    let format = Format::from_uri(&uri);
    let Some(id) = parse_track(params.id.as_deref()) else {
        return response::parameter_error(format, "Required parameter 'id' is missing");
    };
    let task = tokio::spawn(commit_delete(state, id));
    match task.await {
        Ok(Ok(())) => response::empty(format),
        Ok(Err(OperationError::NotFound)) => response::not_found(format),
        Ok(Err(_)) => response::internal(format),
        Err(error) => {
            tracing::error!(%error, "删除 owned operation 异常终止");
            response::internal(format)
        }
    }
}

async fn move_track(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    ApiQuery(params): ApiQuery<MoveParams>,
    _admin: ApiAdmin,
) -> Response {
    let format = Format::from_uri(&uri);
    let Some(id) = parse_track(params.id.as_deref()) else {
        return response::parameter_error(format, "Required parameter 'id' is missing");
    };
    let Some(new_key) = params.key.filter(|value| valid_key(value)) else {
        return response::parameter_error(format, "Required parameter 'key' is missing");
    };
    let task = tokio::spawn(commit_move(state, id, new_key));
    match task.await {
        Ok(Ok(())) => response::empty(format),
        Ok(Err(OperationError::NotFound)) => response::not_found(format),
        Ok(Err(OperationError::DestinationExists)) => {
            response::parameter_error(format, "The destination key already exists")
        }
        Ok(Err(_)) => response::internal(format),
        Err(error) => {
            tracing::error!(%error, "移动 owned operation 异常终止");
            response::internal(format)
        }
    }
}

fn parse_track(value: Option<&str>) -> Option<i64> {
    value.and_then(|value| response::parse_entity_id(value, "track"))
}

fn tag_values(params: &TagParams) -> Vec<(&str, &str)> {
    let mut values = Vec::new();
    for (field, value) in [
        ("title", params.title.as_deref()),
        ("album", params.album.as_deref()),
        ("artist", params.artist.as_deref()),
        ("genre", params.genre.as_deref()),
        ("year", params.year.as_deref()),
        ("track", params.track.as_deref()),
        ("discNumber", params.disc_number.as_deref()),
    ] {
        if let Some(value) = value {
            values.push((field, value));
        }
    }
    values
}

fn valid_tag_numbers(params: &TagParams) -> bool {
    valid_optional_number(params.year.as_deref(), 1, 9999)
        && valid_optional_number(params.track.as_deref(), 1, 999)
        && valid_optional_number(params.disc_number.as_deref(), 1, 999)
}

fn valid_optional_number(value: Option<&str>, min: u32, max: u32) -> bool {
    value.is_none_or(|value| {
        value
            .parse::<u32>()
            .is_ok_and(|number| (min..=max).contains(&number))
    })
}

fn valid_key(value: &str) -> bool {
    value.strip_prefix("library/").is_some_and(|relative| {
        !relative.is_empty() && !relative.starts_with('/') && !relative.contains("..")
    })
}
