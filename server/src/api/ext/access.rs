//! 管理员访问规则扩展：作用域与用户/角色允许名单。

use axum::extract::{OriginalUri, State};
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use contract::{Principal, PrincipalType, ScopeType};
use serde::Deserialize;

use super::super::response::{self, Format};
use super::super::{ApiAdmin, ApiQuery, AppState};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RuleParams {
    scope_type: Option<ScopeType>,
    scope_id: Option<String>,
}

#[derive(Deserialize)]
struct IdParams {
    id: Option<String>,
}

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/rest/ext/setAccessRule", get(set_rule))
        .route("/rest/ext/getAccessRules", get(get_rules))
        .route("/rest/ext/deleteAccessRule", get(delete_rule))
}

async fn set_rule(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    ApiQuery(params): ApiQuery<RuleParams>,
    ApiAdmin(admin): ApiAdmin,
) -> Response {
    let format = Format::from_uri(&uri);
    let (Some(scope_type), Some(raw_scope_id)) = (params.scope_type, params.scope_id) else {
        return response::parameter_error(format, "scopeType and scopeId are required");
    };
    let Some(scope_id) = normalize_scope(scope_type, &raw_scope_id) else {
        return response::parameter_error(format, "scopeId is malformed");
    };
    let grants = match parse_grants(&uri) {
        Ok(value) => value,
        Err(()) => return response::parameter_error(format, "grant is malformed"),
    };
    match scope_exists(&state, scope_type, &scope_id).await {
        Ok(true) => {}
        Ok(false) => return response::not_found(format),
        Err(error) => {
            tracing::error!(%error, "检查访问规则作用域是否存在失败");
            return response::internal(format);
        }
    }
    match principals_exist(&state, &grants).await {
        Ok(true) => {}
        Ok(false) => return response::not_found(format),
        Err(error) => {
            tracing::error!(%error, "检查访问规则授权主体是否存在失败");
            return response::internal(format);
        }
    }
    let id = match state
        .index
        .access()
        .set_rule(scope_type, &scope_id, Some(admin.id), &grants)
        .await
    {
        Ok(value) => value,
        Err(error) => {
            tracing::error!(%error, "设置访问规则失败");
            return response::internal(format);
        }
    };
    let rule = match state.index.access().get_rule(scope_type, &scope_id).await {
        Ok(Some(value)) => value,
        Ok(None) => return response::internal(format),
        Err(error) => {
            tracing::error!(%error, "读取新访问规则失败");
            return response::internal(format);
        }
    };
    debug_assert_eq!(rule.id, id.to_string());
    response::ok(format, serde_json::json!({"accessRule": rule_value(&rule)}))
}

async fn get_rules(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    _admin: ApiAdmin,
) -> Response {
    let format = Format::from_uri(&uri);
    match state.index.access().list_rules().await {
        Ok(rules) => response::ok(
            format,
            serde_json::json!({"accessRules": {
                "accessRule": rules.iter().map(rule_value).collect::<Vec<_>>()
            }}),
        ),
        Err(error) => {
            tracing::error!(%error, "列举访问规则失败");
            response::internal(format)
        }
    }
}

async fn delete_rule(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    ApiQuery(params): ApiQuery<IdParams>,
    _admin: ApiAdmin,
) -> Response {
    let format = Format::from_uri(&uri);
    let Some(id) = params
        .id
        .as_deref()
        .and_then(|value| response::parse_entity_id(value, "rule"))
    else {
        return response::parameter_error(format, "Required parameter 'id' is missing");
    };
    match sqlx::query("DELETE FROM access_rules WHERE id = ?")
        .bind(id)
        .execute(state.index.pool())
        .await
    {
        Ok(result) if result.rows_affected() > 0 => response::empty(format),
        Ok(_) => response::not_found(format),
        Err(error) => {
            tracing::error!(%error, "删除访问规则失败");
            response::internal(format)
        }
    }
}

fn normalize_scope(scope_type: ScopeType, value: &str) -> Option<String> {
    let kind = match scope_type {
        ScopeType::Track => Some("track"),
        ScopeType::Album => Some("album"),
        ScopeType::Artist => Some("artist"),
        ScopeType::Genre => None,
    };
    match kind {
        Some(kind) => response::parse_entity_id(value, kind).map(|id| id.to_string()),
        None if !value.trim().is_empty() => Some(value.to_owned()),
        None => None,
    }
}

fn parse_grants(uri: &axum::http::Uri) -> Result<Vec<Principal>, ()> {
    let mut grants = Vec::new();
    for (name, value) in form_urlencoded::parse(uri.query().unwrap_or_default().as_bytes()) {
        if name != "grant" {
            continue;
        }
        let Some((kind, id)) = value.split_once(':') else {
            return Err(());
        };
        let (principal_type, entity_kind) = match kind {
            "user" => (PrincipalType::User, "user"),
            "role" => (PrincipalType::Role, "role"),
            _ => return Err(()),
        };
        let id = response::parse_entity_id(id, entity_kind).ok_or(())?;
        grants.push(Principal {
            principal_type,
            id: id.to_string(),
        });
    }
    Ok(grants)
}

async fn scope_exists(
    state: &AppState,
    scope_type: ScopeType,
    scope_id: &str,
) -> sqlx::Result<bool> {
    let (sql, value): (&str, &str) = match scope_type {
        ScopeType::Track => ("SELECT COUNT(*) FROM tracks WHERE id = ?", scope_id),
        ScopeType::Album => ("SELECT COUNT(*) FROM albums WHERE id = ?", scope_id),
        ScopeType::Artist => ("SELECT COUNT(*) FROM artists WHERE id = ?", scope_id),
        ScopeType::Genre => (
            "SELECT COUNT(*) FROM tracks WHERE COALESCE((SELECT value FROM tag_overrides o \
             WHERE o.track_id=tracks.id AND o.field='genre'), tracks.genre) = ?",
            scope_id,
        ),
    };
    sqlx::query_scalar::<_, i64>(sql)
        .bind(value)
        .fetch_one(state.index.pool())
        .await
        .map(|count| count > 0)
}

async fn principals_exist(state: &AppState, grants: &[Principal]) -> sqlx::Result<bool> {
    for grant in grants {
        let table = match grant.principal_type {
            PrincipalType::User => "users",
            PrincipalType::Role => "roles",
        };
        let query = format!("SELECT COUNT(*) FROM {table} WHERE id = ?");
        let count = sqlx::query_scalar::<_, i64>(&query)
            .bind(&grant.id)
            .fetch_one(state.index.pool())
            .await?;
        if count == 0 {
            return Ok(false);
        }
    }
    Ok(true)
}

fn rule_value(rule: &contract::AccessRule) -> serde_json::Value {
    let scope_id = match rule.scope_type {
        ScopeType::Track => response::opaque_id("track", &rule.scope_id),
        ScopeType::Album => response::opaque_id("album", &rule.scope_id),
        ScopeType::Artist => response::opaque_id("artist", &rule.scope_id),
        ScopeType::Genre => rule.scope_id.clone(),
    };
    serde_json::json!({
        "id": response::opaque_id("rule", &rule.id),
        "scopeType": rule.scope_type,
        "scopeId": scope_id,
        "grants": rule.grants.iter().map(|grant| serde_json::json!({
            "type": grant.principal_type,
            "id": response::opaque_id(match grant.principal_type {
                PrincipalType::User => "user",
                PrincipalType::Role => "role",
            }, &grant.id)
        })).collect::<Vec<_>>()
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::index::Index;
    use crate::storage::{MemoryStore, ObjectStore};

    use super::*;

    #[tokio::test]
    async fn existence_helpers_propagate_database_errors() {
        let dir = tempfile::tempdir().unwrap();
        let index = Index::connect(&dir.path().join("access.sqlite"))
            .await
            .unwrap();
        let store: Arc<dyn ObjectStore> = Arc::new(MemoryStore::new());
        let state = AppState::new(index.clone(), store, "secret", "/missing/ffmpeg");
        index.pool().close().await;

        assert!(scope_exists(&state, ScopeType::Track, "1").await.is_err());
        assert!(principals_exist(&state, &[]).await.is_ok());
        let grants = [Principal {
            principal_type: PrincipalType::User,
            id: "1".to_owned(),
        }];
        assert!(principals_exist(&state, &grants).await.is_err());
    }
}
