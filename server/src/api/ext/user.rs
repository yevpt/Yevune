//! 管理员读取原生客户端所需的完整用户身份与角色。

use axum::extract::{OriginalUri, State};
use axum::response::Response;
use axum::routing::get;
use axum::Router;

use super::super::response::{self, Format};
use super::super::{ApiAdmin, AppState};

pub(super) fn router() -> Router<AppState> {
    Router::new().route("/rest/ext/getUsers", get(get_users))
}

async fn get_users(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    _admin: ApiAdmin,
) -> Response {
    let format = Format::from_uri(&uri);
    match state.index.users().list_users().await {
        Ok(users) => {
            let users = users
                .into_iter()
                .map(|mut user| {
                    user.id = response::opaque_id("user", &user.id);
                    user
                })
                .collect::<Vec<_>>();
            response::ok(format, serde_json::json!({"users": {"user": users}}))
        }
        Err(error) => {
            tracing::error!(%error, "列举完整用户信息失败");
            response::internal(format)
        }
    }
}
