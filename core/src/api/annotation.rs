//! OpenSubsonic 收藏与评分写操作。

use crate::auth::AuthenticatedSession;
use crate::error::{CoreError, Result};
use crate::http::HttpClient;

/// 可收藏的媒体实体类型。
#[derive(Clone, Copy, uniffi::Enum)]
pub enum AnnotationItemType {
    Track,
    Album,
    Artist,
}

impl AnnotationItemType {
    fn parameter_name(self) -> &'static str {
        match self {
            Self::Track => "id",
            Self::Album => "albumId",
            Self::Artist => "artistId",
        }
    }
}

pub(crate) async fn set_starred(
    http: &HttpClient,
    auth: &AuthenticatedSession,
    id: String,
    item_type: AnnotationItemType,
    starred: bool,
) -> Result<()> {
    let endpoint = if starred { "star" } else { "unstar" };
    http.get_empty_with_params(
        auth,
        endpoint,
        &[(item_type.parameter_name().to_owned(), id)],
    )
    .await
}

pub(crate) async fn set_rating(
    http: &HttpClient,
    auth: &AuthenticatedSession,
    id: String,
    rating: Option<u8>,
) -> Result<()> {
    if rating.is_some_and(|value| !(1..=5).contains(&value)) {
        return Err(CoreError::InvalidRequest {
            message: "rating must be between 1 and 5, or absent to clear".into(),
        });
    }
    http.get_empty_with_params(
        auth,
        "setRating",
        &[
            ("id".to_owned(), id),
            ("rating".to_owned(), rating.unwrap_or(0).to_string()),
        ],
    )
    .await
}
