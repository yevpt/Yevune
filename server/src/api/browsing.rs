//! ID3 语义的艺人、专辑、曲目、流派与索引浏览端点。

use std::collections::BTreeMap;

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
struct AlbumListParams {
    #[serde(rename = "type")]
    list_type: Option<String>,
    size: Option<usize>,
    offset: Option<usize>,
    from_year: Option<u32>,
    to_year: Option<u32>,
    genre: Option<String>,
}

pub fn router() -> Router<AppState> {
    let mut router = Router::new();
    for path in ["/rest/getArtists", "/rest/getArtists.view"] {
        router = router.route(path, get(get_artists));
    }
    for path in ["/rest/getArtist", "/rest/getArtist.view"] {
        router = router.route(path, get(get_artist));
    }
    for path in ["/rest/getAlbum", "/rest/getAlbum.view"] {
        router = router.route(path, get(get_album));
    }
    for path in ["/rest/getSong", "/rest/getSong.view"] {
        router = router.route(path, get(get_song));
    }
    for path in ["/rest/getAlbumList2", "/rest/getAlbumList2.view"] {
        router = router.route(path, get(get_album_list2));
    }
    for path in ["/rest/getGenres", "/rest/getGenres.view"] {
        router = router.route(path, get(get_genres));
    }
    for path in ["/rest/getIndexes", "/rest/getIndexes.view"] {
        router = router.route(path, get(get_indexes));
    }
    router
}

async fn get_artists(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    ApiUser(user): ApiUser,
) -> Response {
    let format = Format::from_uri(&uri);
    let viewer = match state.viewer(user.id).await {
        Ok(viewer) => viewer,
        Err(error) => {
            tracing::error!(%error, "getArtists 解析访问者失败");
            return response::internal(format);
        }
    };
    match state.index.media().list_artists_visible(&viewer).await {
        Ok(mut artists) => {
            if let Err(error) =
                super::annotation::annotate_artists(&state, user.id, &mut artists).await
            {
                tracing::error!(%error, "getArtists 标注查询失败");
                return response::internal(format);
            }
            response::ok(
                format,
                serde_json::json!({"artists": artist_indexes(&artists)}),
            )
        }
        Err(error) => {
            tracing::error!(%error, "getArtists 查询失败");
            response::internal(format)
        }
    }
}

async fn get_artist(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    ApiQuery(params): ApiQuery<IdParams>,
    ApiUser(user): ApiUser,
) -> Response {
    let format = Format::from_uri(&uri);
    let Some(id) = params
        .id
        .as_deref()
        .and_then(|id| response::parse_entity_id(id, "artist"))
    else {
        return response::parameter_error(format, "Required parameter 'id' is missing");
    };
    let viewer = match state.viewer(user.id).await {
        Ok(viewer) => viewer,
        Err(error) => {
            tracing::error!(%error, "getArtist 解析访问者失败");
            return response::internal(format);
        }
    };
    let mut artist = match state.index.media().get_artist_visible(&viewer, id).await {
        Ok(Some(artist)) => artist,
        Ok(None) => return response::not_found(format),
        Err(error) => {
            tracing::error!(%error, "getArtist 查询失败");
            return response::internal(format);
        }
    };
    let mut albums = match state.index.media().artist_albums_visible(&viewer, id).await {
        Ok(albums) => albums,
        Err(error) => {
            tracing::error!(%error, "getArtist 专辑查询失败");
            return response::internal(format);
        }
    };
    if let Err(error) =
        super::annotation::annotate_artists(&state, user.id, std::slice::from_mut(&mut artist))
            .await
    {
        tracing::error!(%error, "getArtist 艺人标注查询失败");
        return response::internal(format);
    }
    if let Err(error) = super::annotation::annotate_albums(&state, user.id, &mut albums).await {
        tracing::error!(%error, "getArtist 专辑标注查询失败");
        return response::internal(format);
    }
    let mut value = response::artist_value(&artist);
    value.as_object_mut().expect("artist 是对象").insert(
        "album".into(),
        albums
            .iter()
            .map(response::album_value)
            .collect::<Vec<_>>()
            .into(),
    );
    response::ok(format, serde_json::json!({"artist": value}))
}

async fn get_album(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    ApiQuery(params): ApiQuery<IdParams>,
    ApiUser(user): ApiUser,
) -> Response {
    let format = Format::from_uri(&uri);
    let Some(id) = params
        .id
        .as_deref()
        .and_then(|id| response::parse_entity_id(id, "album"))
    else {
        return response::parameter_error(format, "Required parameter 'id' is missing");
    };
    let viewer = match state.viewer(user.id).await {
        Ok(viewer) => viewer,
        Err(error) => {
            tracing::error!(%error, "getAlbum 解析访问者失败");
            return response::internal(format);
        }
    };
    let mut album = match state.index.media().get_album_visible(&viewer, id).await {
        Ok(Some(album)) => album,
        Ok(None) => return response::not_found(format),
        Err(error) => {
            tracing::error!(%error, "getAlbum 查询失败");
            return response::internal(format);
        }
    };
    let mut tracks = match state.index.media().album_tracks_visible(&viewer, id).await {
        Ok(tracks) => tracks,
        Err(error) => {
            tracing::error!(%error, "getAlbum 曲目查询失败");
            return response::internal(format);
        }
    };
    if let Err(error) =
        super::annotation::annotate_albums(&state, user.id, std::slice::from_mut(&mut album)).await
    {
        tracing::error!(%error, "getAlbum 专辑标注查询失败");
        return response::internal(format);
    }
    if let Err(error) = super::annotation::annotate_tracks(&state, user.id, &mut tracks).await {
        tracing::error!(%error, "getAlbum 曲目标注查询失败");
        return response::internal(format);
    }
    let mut value = response::album_value(&album);
    value.as_object_mut().expect("album 是对象").insert(
        "song".into(),
        tracks
            .iter()
            .map(response::track_value)
            .collect::<Vec<_>>()
            .into(),
    );
    response::ok(format, serde_json::json!({"album": value}))
}

async fn get_song(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    ApiQuery(params): ApiQuery<IdParams>,
    ApiUser(user): ApiUser,
) -> Response {
    let format = Format::from_uri(&uri);
    let Some(id) = params
        .id
        .as_deref()
        .and_then(|id| response::parse_entity_id(id, "track"))
    else {
        return response::parameter_error(format, "Required parameter 'id' is missing");
    };
    let viewer = match state.viewer(user.id).await {
        Ok(viewer) => viewer,
        Err(error) => {
            tracing::error!(%error, "getSong 解析访问者失败");
            return response::internal(format);
        }
    };
    match state.index.media().get_track_visible(&viewer, id).await {
        Ok(Some(mut track)) => {
            if let Err(error) = super::annotation::annotate_tracks(
                &state,
                user.id,
                std::slice::from_mut(&mut track),
            )
            .await
            {
                tracing::error!(%error, "getSong 标注查询失败");
                return response::internal(format);
            }
            response::ok(
                format,
                serde_json::json!({"song": response::track_value(&track)}),
            )
        }
        Ok(None) => response::not_found(format),
        Err(error) => {
            tracing::error!(%error, "getSong 查询失败");
            response::internal(format)
        }
    }
}

async fn get_album_list2(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    ApiQuery(params): ApiQuery<AlbumListParams>,
    ApiUser(user): ApiUser,
) -> Response {
    let format = Format::from_uri(&uri);
    let Some(list_type) = params.list_type.as_deref() else {
        return response::parameter_error(format, "Required parameter 'type' is missing");
    };
    let valid = [
        "random",
        "newest",
        "highest",
        "frequent",
        "recent",
        "alphabeticalByName",
        "alphabeticalByArtist",
        "starred",
        "byYear",
        "byGenre",
    ];
    if !valid.contains(&list_type) {
        return response::parameter_error(format, "Invalid album list type");
    }
    if list_type == "byYear" && (params.from_year.is_none() || params.to_year.is_none()) {
        return response::parameter_error(format, "fromYear and toYear are required");
    }
    if list_type == "byGenre" && params.genre.is_none() {
        return response::parameter_error(format, "genre is required");
    }
    let viewer = match state.viewer(user.id).await {
        Ok(viewer) => viewer,
        Err(error) => {
            tracing::error!(%error, "getAlbumList2 解析访问者失败");
            return response::internal(format);
        }
    };
    let offset = params.offset.unwrap_or(0) as i64;
    let size = params.size.unwrap_or(10).min(500) as i64;
    let ids = match state
        .index
        .media()
        .album_ids_for_list(
            &viewer,
            list_type,
            offset,
            size,
            params.from_year,
            params.to_year,
            params.genre.as_deref(),
        )
        .await
    {
        Ok(ids) => ids,
        Err(error) => {
            tracing::error!(%error, "getAlbumList2 查询失败");
            return response::internal(format);
        }
    };
    let mut albums = Vec::with_capacity(ids.len());
    for id in ids {
        match state.index.media().get_album_visible(&viewer, id).await {
            Ok(Some(album)) => albums.push(album),
            Ok(None) => {}
            Err(error) => {
                tracing::error!(%error, "getAlbumList2 专辑读取失败");
                return response::internal(format);
            }
        }
    }
    if let Err(error) = super::annotation::annotate_albums(&state, user.id, &mut albums).await {
        tracing::error!(%error, "getAlbumList2 标注查询失败");
        return response::internal(format);
    }
    let values = albums.iter().map(response::album_value).collect::<Vec<_>>();
    response::ok(format, serde_json::json!({"albumList2": {"album": values}}))
}

async fn get_genres(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    ApiUser(user): ApiUser,
) -> Response {
    let format = Format::from_uri(&uri);
    let viewer = match state.viewer(user.id).await {
        Ok(viewer) => viewer,
        Err(error) => {
            tracing::error!(%error, "getGenres 解析访问者失败");
            return response::internal(format);
        }
    };
    match state.index.media().list_genres_visible(&viewer).await {
        Ok(genres) => response::ok(format, serde_json::json!({"genres": {"genre": genres}})),
        Err(error) => {
            tracing::error!(%error, "getGenres 查询失败");
            response::internal(format)
        }
    }
}

async fn get_indexes(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    ApiUser(user): ApiUser,
) -> Response {
    let format = Format::from_uri(&uri);
    let viewer = match state.viewer(user.id).await {
        Ok(viewer) => viewer,
        Err(error) => {
            tracing::error!(%error, "getIndexes 解析访问者失败");
            return response::internal(format);
        }
    };
    match state.index.media().list_artists_visible(&viewer).await {
        Ok(mut artists) => {
            if let Err(error) =
                super::annotation::annotate_artists(&state, user.id, &mut artists).await
            {
                tracing::error!(%error, "getIndexes 标注查询失败");
                return response::internal(format);
            }
            let mut indexes = artist_indexes(&artists);
            indexes
                .as_object_mut()
                .expect("indexes 是对象")
                .insert("lastModified".into(), 0.into());
            response::ok(format, serde_json::json!({"indexes": indexes}))
        }
        Err(error) => {
            tracing::error!(%error, "getIndexes 查询失败");
            response::internal(format)
        }
    }
}

fn artist_indexes(artists: &[contract::Artist]) -> serde_json::Value {
    let mut groups: BTreeMap<String, Vec<serde_json::Value>> = BTreeMap::new();
    for artist in artists {
        let name = artist
            .name
            .chars()
            .next()
            .map(|value| value.to_uppercase().to_string())
            .unwrap_or_else(|| "#".to_string());
        groups
            .entry(name)
            .or_default()
            .push(response::artist_value(artist));
    }
    let indexes: Vec<_> = groups
        .into_iter()
        .map(|(name, artist)| serde_json::json!({"name": name, "artist": artist}))
        .collect();
    serde_json::json!({"ignoredArticles": "The El La Los Las Le Les", "index": indexes})
}
