//! 曲库访问规则扩展 API。

use contract::{AccessRule, Principal, PrincipalType, ScopeType};
use serde::Deserialize;

use crate::auth::AuthenticatedSession;
use crate::error::Result;
use crate::http::HttpClient;

pub(crate) async fn list_access_rules(
    http: &HttpClient,
    auth: &AuthenticatedSession,
) -> Result<Vec<AccessRule>> {
    let payload: AccessRulesPayload = http.get_json(auth, "ext/getAccessRules", &[]).await?;
    Ok(payload.access_rules.access_rule)
}

pub(crate) async fn set_access_rule(
    http: &HttpClient,
    auth: &AuthenticatedSession,
    scope_type: ScopeType,
    scope_id: String,
    grants: Vec<Principal>,
) -> Result<AccessRule> {
    let scope = match scope_type {
        ScopeType::Track => "track",
        ScopeType::Album => "album",
        ScopeType::Artist => "artist",
        ScopeType::Genre => "genre",
    };
    let mut parameters = vec![
        ("scopeType".to_owned(), scope.to_owned()),
        ("scopeId".to_owned(), scope_id),
    ];
    parameters.extend(grants.into_iter().map(|grant| {
        let kind = match grant.principal_type {
            PrincipalType::User => "user",
            PrincipalType::Role => "role",
        };
        ("grant".to_owned(), format!("{kind}:{}", grant.id))
    }));

    let payload: AccessRulePayload = http
        .get_json(auth, "ext/setAccessRule", &parameters)
        .await?;
    Ok(payload.access_rule)
}

pub(crate) async fn delete_access_rule(
    http: &HttpClient,
    auth: &AuthenticatedSession,
    id: String,
) -> Result<()> {
    http.get_empty_with_params(auth, "ext/deleteAccessRule", &[("id".to_owned(), id)])
        .await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccessRulesPayload {
    access_rules: AccessRulesBody,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccessRulesBody {
    #[serde(default)]
    access_rule: Vec<AccessRule>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccessRulePayload {
    access_rule: AccessRule,
}
