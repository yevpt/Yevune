//! 收藏、评分与播放记录端点，全部按当前用户隔离。

use axum::extract::{OriginalUri, State};
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use serde::Deserialize;

use super::response::{self, Format};
use super::{ApiQuery, ApiUser, AppState};

#[derive(Deserialize)]
struct RatingParams {
    id: Option<String>,
    rating: Option<u8>,
}

#[derive(Deserialize)]
struct ScrobbleParams {
    submission: Option<bool>,
}

pub fn router() -> Router<AppState> {
    let mut router = Router::new();
    for path in ["/rest/star", "/rest/star.view"] {
        router = router.route(path, get(star));
    }
    for path in ["/rest/unstar", "/rest/unstar.view"] {
        router = router.route(path, get(unstar));
    }
    for path in ["/rest/setRating", "/rest/setRating.view"] {
        router = router.route(path, get(set_rating));
    }
    for path in ["/rest/scrobble", "/rest/scrobble.view"] {
        router = router.route(path, get(scrobble));
    }
    router
}

async fn star(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    ApiUser(user): ApiUser,
) -> Response {
    annotate_items(&state, Format::from_uri(&uri), user.id, &uri, true).await
}

async fn unstar(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    ApiUser(user): ApiUser,
) -> Response {
    annotate_items(&state, Format::from_uri(&uri), user.id, &uri, false).await
}

async fn annotate_items(
    state: &AppState,
    format: Format,
    user_id: i64,
    uri: &axum::http::Uri,
    starred: bool,
) -> Response {
    let groups = [
        ("track", "id"),
        ("album", "albumId"),
        ("artist", "artistId"),
    ];
    let mut items = Vec::new();
    for (kind, name) in groups {
        let ids = match response::query_entity_ids(uri, name, kind) {
            Ok(ids) => ids,
            Err(()) => return response::parameter_error(format, "Item id is malformed"),
        };
        items.extend(ids.into_iter().map(|id| (kind, id)));
    }
    if items.is_empty() {
        return response::parameter_error(format, "At least one item id is required");
    }
    for (kind, id) in items {
        let result = if starred {
            state.index.annotations().star(user_id, kind, id).await
        } else {
            state.index.annotations().unstar(user_id, kind, id).await
        };
        if let Err(error) = result {
            tracing::error!(%error, "star/unstar 写入失败");
            return response::internal(format);
        }
    }
    response::empty(format)
}

async fn set_rating(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    ApiQuery(params): ApiQuery<RatingParams>,
    ApiUser(user): ApiUser,
) -> Response {
    let format = Format::from_uri(&uri);
    let (Some(raw_id), Some(rating)) = (params.id, params.rating) else {
        return response::parameter_error(format, "id and rating are required");
    };
    let Some((kind, id)) = response::parse_ratable_id(&raw_id) else {
        return response::parameter_error(format, "id is malformed");
    };
    if rating > 5 {
        return response::parameter_error(format, "rating must be between 0 and 5");
    }
    let value = (rating != 0).then_some(rating);
    match state
        .index
        .annotations()
        .set_rating(user.id, kind, id, value)
        .await
    {
        Ok(()) => response::empty(format),
        Err(error) => {
            tracing::error!(%error, "setRating 写入失败");
            response::internal(format)
        }
    }
}

async fn scrobble(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    ApiQuery(params): ApiQuery<ScrobbleParams>,
    ApiUser(user): ApiUser,
) -> Response {
    let format = Format::from_uri(&uri);
    let ids = match response::query_entity_ids(&uri, "id", "track") {
        Ok(ids) if !ids.is_empty() => ids,
        Ok(_) => return response::parameter_error(format, "Required parameter 'id' is missing"),
        Err(()) => return response::parameter_error(format, "id is malformed"),
    };
    let times = match response::query_i64_values(&uri, "time") {
        Ok(times) if times.is_empty() || times.len() == ids.len() => times,
        Ok(_) => return response::parameter_error(format, "time count must match id count"),
        Err(()) => return response::parameter_error(format, "time is malformed"),
    };
    if params.submission == Some(false) {
        return response::empty(format);
    }
    for (index, id) in ids.into_iter().enumerate() {
        if let Err(error) = state
            .index
            .annotations()
            .scrobble_at(user.id, "track", id, times.get(index).copied())
            .await
        {
            tracing::error!(%error, "scrobble 写入失败");
            return response::internal(format);
        }
    }
    response::empty(format)
}
