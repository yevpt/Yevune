//! OpenSubsonic `search3`，由 SQLite FTS5 驱动。

use axum::extract::{OriginalUri, State};
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use serde::Deserialize;

use super::response::{self, Format};
use super::{ApiQuery, ApiUser, AppState};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Params {
    query: Option<String>,
    artist_count: Option<usize>,
    artist_offset: Option<usize>,
    album_count: Option<usize>,
    album_offset: Option<usize>,
    song_count: Option<usize>,
    song_offset: Option<usize>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/rest/search3", get(search3))
        .route("/rest/search3.view", get(search3))
}

async fn search3(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    ApiQuery(params): ApiQuery<Params>,
    ApiUser(user): ApiUser,
) -> Response {
    let format = Format::from_uri(&uri);
    let Some(query) = params.query else {
        return response::parameter_error(format, "Required parameter 'query' is missing");
    };
    let empty_query = query.trim().is_empty();
    let artist_offset = params.artist_offset.unwrap_or(0);
    let artist_count = params.artist_count.unwrap_or(20).min(500);
    let album_offset = params.album_offset.unwrap_or(0);
    let album_count = params.album_count.unwrap_or(20).min(500);
    let song_offset = params.song_offset.unwrap_or(0);
    let song_count = params.song_count.unwrap_or(20).min(500);
    // 访问控制强制：先解析访问者，再只取其可见的命中；分页在过滤后的可见集上切片。
    let viewer = match state.viewer(user.id).await {
        Ok(viewer) => viewer,
        Err(error) => {
            tracing::error!(%error, "search3 解析访问者失败");
            return response::internal(format);
        }
    };
    let media = state.index.media();
    let search = if empty_query {
        match (
            media.list_artists_visible(&viewer).await,
            media.list_albums_visible(&viewer).await,
            media.list_tracks_visible(&viewer).await,
        ) {
            (Ok(artists), Ok(albums), Ok(tracks)) => Ok(crate::index::SearchResults {
                artists,
                albums,
                tracks,
            }),
            (Err(error), _, _) | (_, Err(error), _) | (_, _, Err(error)) => Err(error),
        }
    } else {
        // 过滤后再切片，故一次取足各类型 offset+count 之和的可见命中。
        let limit = (artist_offset + artist_count)
            .max(album_offset + album_count)
            .max(song_offset + song_count)
            .saturating_mul(3)
            .max(1) as i64;
        media.search_visible(&viewer, &query, limit).await
    };
    match search {
        Ok(mut results) => {
            results.artists = page(results.artists, artist_offset, artist_count);
            results.albums = page(results.albums, album_offset, album_count);
            results.tracks = page(results.tracks, song_offset, song_count);
            let annotations = async {
                super::annotation::annotate_artists(&state, user.id, &mut results.artists).await?;
                super::annotation::annotate_albums(&state, user.id, &mut results.albums).await?;
                super::annotation::annotate_tracks(&state, user.id, &mut results.tracks).await
            }
            .await;
            if let Err(error) = annotations {
                tracing::error!(%error, "search3 标注查询失败");
                return response::internal(format);
            }
            let artists: Vec<_> = results.artists.iter().map(response::artist_value).collect();
            let albums: Vec<_> = results.albums.iter().map(response::album_value).collect();
            let songs: Vec<_> = results.tracks.iter().map(response::track_value).collect();
            response::ok(
                format,
                serde_json::json!({
                    "searchResult3": {"artist": artists, "album": albums, "song": songs}
                }),
            )
        }
        Err(error) => {
            tracing::error!(%error, "search3 查询失败");
            response::internal(format)
        }
    }
}

/// 在可见集上按 `offset`/`count` 切片（在访问控制过滤之后分页）。
fn page<T>(items: Vec<T>, offset: usize, count: usize) -> Vec<T> {
    items.into_iter().skip(offset).take(count).collect()
}
