//! 管理员流式替换专辑封面。

use axum::extract::{DefaultBodyLimit, Multipart, OriginalUri, State};
use axum::response::Response;
use axum::routing::post;
use axum::Router;
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::super::response::{self, Format};
use super::super::{ApiAdmin, AppState};

const MAX_COVER_BYTES: usize = 32 * 1024 * 1024;

pub(super) fn router() -> Router<AppState> {
    Router::new().route(
        "/rest/ext/setCoverArt",
        post(set_cover_art).layer(DefaultBodyLimit::max(MAX_COVER_BYTES)),
    )
}

async fn set_cover_art(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    _admin: ApiAdmin,
    mut multipart: Multipart,
) -> Response {
    let format = Format::from_uri(&uri);
    let mut id = None;
    let mut temp = None;
    while let Ok(Some(field)) = multipart.next_field().await {
        match field.name() {
            Some("id") => id = field.text().await.ok(),
            Some("file") => {
                let file = match NamedTempFile::new() {
                    Ok(file) => file,
                    Err(_) => return response::internal(format),
                };
                let mut output = match tokio::fs::File::create(file.path()).await {
                    Ok(output) => output,
                    Err(_) => return response::internal(format),
                };
                let mut field = field;
                while let Ok(Some(chunk)) = field.chunk().await {
                    if output.write_all(&chunk).await.is_err() {
                        return response::internal(format);
                    }
                }
                if output.flush().await.is_err() {
                    return response::internal(format);
                }
                temp = Some(file);
            }
            _ => {}
        }
    }
    let Some(album_id) = id
        .as_deref()
        .and_then(|value| response::parse_entity_id(value, "album"))
    else {
        return response::parameter_error(format, "album id is required");
    };
    let Some(temp) = temp else {
        return response::parameter_error(format, "file is required");
    };
    let key = match hash_key(temp.path()).await {
        Ok(key) => key,
        Err(_) => return response::internal(format),
    };
    if state.store.put_file(&key, temp.path()).await.is_err() {
        return response::internal(format);
    }
    match state.index.media().set_album_cover(album_id, &key).await {
        Ok(true) => response::empty(format),
        Ok(false) => {
            let _ = state.store.delete(&key).await;
            response::not_found(format)
        }
        Err(_) => {
            let _ = state.store.delete(&key).await;
            response::internal(format)
        }
    }
}

async fn hash_key(path: &std::path::Path) -> std::io::Result<String> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let count = file.read(&mut buffer).await?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("covers/{}.img", hex::encode(hash.finalize())))
}
