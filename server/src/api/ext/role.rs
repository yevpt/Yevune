//! 管理员角色 CRUD 与用户角色分配扩展。

use axum::extract::{OriginalUri, State};
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use serde::Deserialize;

use super::super::response::{self, Format};
use super::super::{ApiAdmin, ApiQuery, AppState};

#[derive(Deserialize)]
struct NameParams {
    name: Option<String>,
}

#[derive(Deserialize)]
struct IdParams {
    id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AssignmentParams {
    user_id: Option<String>,
    role_id: Option<String>,
}

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/rest/ext/getRoles", get(get_roles))
        .route("/rest/ext/createRole", get(create_role))
        .route("/rest/ext/deleteRole", get(delete_role))
        .route("/rest/ext/assignRole", get(assign_role))
        .route("/rest/ext/unassignRole", get(unassign_role))
}

async fn get_roles(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    _admin: ApiAdmin,
) -> Response {
    let format = Format::from_uri(&uri);
    match state.index.roles().list_roles().await {
        Ok(roles) => response::ok(
            format,
            serde_json::json!({"roles": {
                "role": roles.iter().map(role_value).collect::<Vec<_>>()
            }}),
        ),
        Err(error) => {
            tracing::error!(%error, "列举角色失败");
            response::internal(format)
        }
    }
}

async fn create_role(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    ApiQuery(params): ApiQuery<NameParams>,
    _admin: ApiAdmin,
) -> Response {
    let format = Format::from_uri(&uri);
    let Some(name) = params.name.filter(|name| !name.trim().is_empty()) else {
        return response::parameter_error(format, "Required parameter 'name' is missing");
    };
    let id = match state.index.roles().create_role(name.trim(), false).await {
        Ok(value) => value,
        Err(error) => {
            tracing::error!(%error, "创建角色失败");
            return response::internal(format);
        }
    };
    response::ok(
        format,
        serde_json::json!({"role": {
            "id": response::opaque_id("role", &id.to_string()),
            "name": name.trim(),
            "isBuiltin": false
        }}),
    )
}

async fn delete_role(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    ApiQuery(params): ApiQuery<IdParams>,
    _admin: ApiAdmin,
) -> Response {
    let format = Format::from_uri(&uri);
    let Some(id) = params
        .id
        .as_deref()
        .and_then(|value| response::parse_entity_id(value, "role"))
    else {
        return response::parameter_error(format, "Required parameter 'id' is missing");
    };
    let role = match state.index.roles().list_roles().await {
        Ok(roles) => roles.into_iter().find(|role| role.id == id.to_string()),
        Err(error) => {
            tracing::error!(%error, "读取待删角色失败");
            return response::internal(format);
        }
    };
    let Some(role) = role else {
        return response::not_found(format);
    };
    if role.is_builtin {
        return response::auth_error(format, crate::auth::AuthError::Forbidden);
    }
    match state.index.roles().delete_role(id).await {
        Ok(true) => response::empty(format),
        Ok(false) => response::not_found(format),
        Err(error) => {
            tracing::error!(%error, "删除角色失败");
            response::internal(format)
        }
    }
}

async fn assign_role(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    ApiQuery(params): ApiQuery<AssignmentParams>,
    _admin: ApiAdmin,
) -> Response {
    assignment(state, Format::from_uri(&uri), params, true).await
}

async fn unassign_role(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    ApiQuery(params): ApiQuery<AssignmentParams>,
    _admin: ApiAdmin,
) -> Response {
    assignment(state, Format::from_uri(&uri), params, false).await
}

async fn assignment(
    state: AppState,
    format: Format,
    params: AssignmentParams,
    assign: bool,
) -> Response {
    let Some(user_id) = params
        .user_id
        .as_deref()
        .and_then(|value| response::parse_entity_id(value, "user"))
    else {
        return response::parameter_error(format, "userId is malformed");
    };
    let Some(role_id) = params
        .role_id
        .as_deref()
        .and_then(|value| response::parse_entity_id(value, "role"))
    else {
        return response::parameter_error(format, "roleId is malformed");
    };
    match assignment_entities_exist(&state, user_id, role_id).await {
        Ok(true) => {}
        Ok(false) => return response::not_found(format),
        Err(error) => {
            tracing::error!(%error, "检查角色分配实体是否存在失败");
            return response::internal(format);
        }
    }
    let result = if assign {
        state
            .index
            .roles()
            .assign(user_id, role_id)
            .await
            .map(|_| ())
    } else {
        state
            .index
            .roles()
            .unassign(user_id, role_id)
            .await
            .map(|_| ())
    };
    match result {
        Ok(()) => response::empty(format),
        Err(error) => {
            tracing::error!(%error, "分配或解除角色失败");
            response::internal(format)
        }
    }
}

async fn assignment_entities_exist(
    state: &AppState,
    user_id: i64,
    role_id: i64,
) -> sqlx::Result<bool> {
    let user_exists = state.index.users().get_user(user_id).await?.is_some();
    if !user_exists {
        return Ok(false);
    }
    state
        .index
        .roles()
        .list_roles()
        .await
        .map(|roles| roles.iter().any(|role| role.id == role_id.to_string()))
}

fn role_value(role: &contract::Role) -> serde_json::Value {
    serde_json::json!({
        "id": response::opaque_id("role", &role.id),
        "name": role.name,
        "isBuiltin": role.is_builtin
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::index::Index;
    use crate::storage::{MemoryStore, ObjectStore};

    use super::*;

    #[tokio::test]
    async fn assignment_existence_propagates_database_errors() {
        let dir = tempfile::tempdir().unwrap();
        let index = Index::connect(&dir.path().join("role.sqlite"))
            .await
            .unwrap();
        let store: Arc<dyn ObjectStore> = Arc::new(MemoryStore::new());
        let state = AppState::new(index.clone(), store, "secret", "/missing/ffmpeg");
        index.pool().close().await;

        assert!(assignment_entities_exist(&state, 1, 1).await.is_err());
    }
}
