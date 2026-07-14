//! 管理员用户与角色 API 编排。

use contract::{Role, User};
use serde::Deserialize;

use crate::auth::AuthenticatedSession;
use crate::error::Result;
use crate::http::HttpClient;

pub(crate) async fn current_user_is_admin(
    http: &HttpClient,
    auth: &AuthenticatedSession,
) -> Result<bool> {
    let payload: CurrentUserPayload = http
        .get_json(
            auth,
            "getUser",
            &[("username".to_owned(), auth.user.clone())],
        )
        .await?;
    Ok(payload.user.admin_role)
}

pub(crate) async fn list_users(
    http: &HttpClient,
    auth: &AuthenticatedSession,
) -> Result<Vec<User>> {
    let payload: UsersPayload = http.get_json(auth, "ext/getUsers", &[]).await?;
    Ok(payload.users.user)
}

pub(crate) async fn list_roles(
    http: &HttpClient,
    auth: &AuthenticatedSession,
) -> Result<Vec<Role>> {
    let payload: RolesPayload = http.get_json(auth, "ext/getRoles", &[]).await?;
    Ok(payload.roles.role)
}

#[derive(Deserialize)]
struct CurrentUserPayload {
    user: CurrentUser,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurrentUser {
    admin_role: bool,
}

#[derive(Deserialize)]
struct UsersPayload {
    users: UsersBody,
}

#[derive(Deserialize)]
struct UsersBody {
    #[serde(default)]
    user: Vec<User>,
}

#[derive(Deserialize)]
struct RolesPayload {
    roles: RolesBody,
}

#[derive(Deserialize)]
struct RolesBody {
    #[serde(default)]
    role: Vec<Role>,
}
