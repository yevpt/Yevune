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

pub(crate) async fn create_user(
    http: &HttpClient,
    auth: &AuthenticatedSession,
    username: String,
    email: String,
    password: String,
    admin: bool,
) -> Result<()> {
    http.get_empty_with_params(
        auth,
        "createUser",
        &[
            ("username".to_owned(), username),
            ("email".to_owned(), email),
            ("password".to_owned(), password),
            ("adminRole".to_owned(), admin.to_string()),
        ],
    )
    .await
}

pub(crate) async fn update_user(
    http: &HttpClient,
    auth: &AuthenticatedSession,
    username: String,
    email: String,
    admin: bool,
) -> Result<()> {
    http.get_empty_with_params(
        auth,
        "updateUser",
        &[
            ("username".to_owned(), username),
            ("email".to_owned(), email),
            ("adminRole".to_owned(), admin.to_string()),
        ],
    )
    .await
}

pub(crate) async fn change_password(
    http: &HttpClient,
    auth: &AuthenticatedSession,
    username: String,
    password: String,
) -> Result<()> {
    http.get_empty_with_params(
        auth,
        "changePassword",
        &[
            ("username".to_owned(), username),
            ("password".to_owned(), password),
        ],
    )
    .await
}

pub(crate) async fn delete_user(
    http: &HttpClient,
    auth: &AuthenticatedSession,
    username: String,
) -> Result<()> {
    http.get_empty_with_params(auth, "deleteUser", &[("username".to_owned(), username)])
        .await
}

pub(crate) async fn create_role(
    http: &HttpClient,
    auth: &AuthenticatedSession,
    name: String,
) -> Result<Role> {
    let payload: RolePayload = http
        .get_json(auth, "ext/createRole", &[("name".to_owned(), name)])
        .await?;
    Ok(payload.role)
}

pub(crate) async fn delete_role(
    http: &HttpClient,
    auth: &AuthenticatedSession,
    id: String,
) -> Result<()> {
    http.get_empty_with_params(auth, "ext/deleteRole", &[("id".to_owned(), id)])
        .await
}

pub(crate) async fn assign_role(
    http: &HttpClient,
    auth: &AuthenticatedSession,
    user_id: String,
    role_id: String,
) -> Result<()> {
    http.get_empty_with_params(
        auth,
        "ext/assignRole",
        &[
            ("userId".to_owned(), user_id),
            ("roleId".to_owned(), role_id),
        ],
    )
    .await
}

pub(crate) async fn unassign_role(
    http: &HttpClient,
    auth: &AuthenticatedSession,
    user_id: String,
    role_id: String,
) -> Result<()> {
    http.get_empty_with_params(
        auth,
        "ext/unassignRole",
        &[
            ("userId".to_owned(), user_id),
            ("roleId".to_owned(), role_id),
        ],
    )
    .await
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

#[derive(Deserialize)]
struct RolePayload {
    role: Role,
}
