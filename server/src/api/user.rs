//! OpenSubsonic 用户管理端点，全部通过现有 admin 角色机制强制授权。

use axum::extract::{OriginalUri, State};
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use serde::Deserialize;

use crate::auth::UserAdmin;

use super::response::{self, Format};
use super::{ApiAdmin, ApiQuery, ApiUser, AppState};

#[derive(Deserialize)]
struct UsernameParams {
    username: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateParams {
    username: Option<String>,
    password: Option<String>,
    email: Option<String>,
    admin_role: Option<bool>,
    #[allow(dead_code)]
    settings_role: Option<bool>,
    #[allow(dead_code)]
    stream_role: Option<bool>,
    #[allow(dead_code)]
    download_role: Option<bool>,
    #[allow(dead_code)]
    upload_role: Option<bool>,
    #[allow(dead_code)]
    playlist_role: Option<bool>,
    #[allow(dead_code)]
    cover_art_role: Option<bool>,
    #[allow(dead_code)]
    comment_role: Option<bool>,
    jukebox_role: Option<bool>,
    podcast_role: Option<bool>,
    share_role: Option<bool>,
    video_conversion_role: Option<bool>,
    ldap_authenticated: Option<bool>,
    max_bit_rate: Option<u32>,
    music_folder_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateParams {
    username: Option<String>,
    password: Option<String>,
    admin_role: Option<bool>,
    email: Option<String>,
    settings_role: Option<bool>,
    stream_role: Option<bool>,
    download_role: Option<bool>,
    upload_role: Option<bool>,
    playlist_role: Option<bool>,
    cover_art_role: Option<bool>,
    comment_role: Option<bool>,
    jukebox_role: Option<bool>,
    podcast_role: Option<bool>,
    share_role: Option<bool>,
    video_conversion_role: Option<bool>,
    ldap_authenticated: Option<bool>,
    max_bit_rate: Option<u32>,
    music_folder_id: Option<String>,
}

#[derive(Deserialize)]
struct ChangePasswordParams {
    username: Option<String>,
    password: Option<String>,
}

pub fn router() -> Router<AppState> {
    let mut router = Router::new();
    for path in ["/rest/getUser", "/rest/getUser.view"] {
        router = router.route(path, get(get_user));
    }
    for path in ["/rest/getUsers", "/rest/getUsers.view"] {
        router = router.route(path, get(get_users));
    }
    for path in ["/rest/createUser", "/rest/createUser.view"] {
        router = router.route(path, get(create_user));
    }
    for path in ["/rest/updateUser", "/rest/updateUser.view"] {
        router = router.route(path, get(update_user));
    }
    for path in ["/rest/deleteUser", "/rest/deleteUser.view"] {
        router = router.route(path, get(delete_user));
    }
    for path in ["/rest/changePassword", "/rest/changePassword.view"] {
        router = router.route(path, get(change_password));
    }
    router
}

async fn get_user(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    ApiQuery(params): ApiQuery<UsernameParams>,
    ApiUser(caller): ApiUser,
) -> Response {
    let format = Format::from_uri(&uri);
    let Some(username) = params.username.filter(|name| !name.is_empty()) else {
        return response::parameter_error(format, "Required parameter 'username' is missing");
    };
    if !caller.admin && caller.name != username {
        return response::auth_error(format, crate::auth::AuthError::Forbidden);
    }
    match state.index.users().get_user_by_name(&username).await {
        Ok(Some(user)) => response::ok(
            format,
            serde_json::json!({"user": response::user_value(&user)}),
        ),
        Ok(None) => response::not_found(format),
        Err(error) => {
            tracing::error!(%error, "getUser 查询失败");
            response::internal(format)
        }
    }
}

async fn get_users(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    ApiAdmin(_admin): ApiAdmin,
) -> Response {
    let format = Format::from_uri(&uri);
    match state.index.users().list_users().await {
        Ok(users) => {
            let values: Vec<_> = users.iter().map(response::user_value).collect();
            response::ok(format, serde_json::json!({"users": {"user": values}}))
        }
        Err(error) => {
            tracing::error!(%error, "getUsers 查询失败");
            response::internal(format)
        }
    }
}

async fn create_user(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    ApiQuery(params): ApiQuery<CreateParams>,
    ApiAdmin(_admin): ApiAdmin,
) -> Response {
    let format = Format::from_uri(&uri);
    let (Some(username), Some(password), Some(email)) =
        (params.username, params.password, params.email)
    else {
        return response::parameter_error(format, "username, password and email are required");
    };
    let password = match decode_password(&password) {
        Some(password) => password,
        None => return response::parameter_error(format, "password is malformed"),
    };
    if !permissions_supported(
        params.admin_role.unwrap_or(false),
        params.settings_role,
        params.stream_role,
        params.download_role,
        params.upload_role,
        params.playlist_role,
        params.cover_art_role,
        params.comment_role,
        params.jukebox_role,
        params.podcast_role,
        params.share_role,
        params.video_conversion_role,
        params.ldap_authenticated,
        params.max_bit_rate,
        params.music_folder_id.as_deref(),
    ) {
        return response::error(format, 0, "Requested user permissions are not supported");
    }
    let admin = UserAdmin::new(&state.index, &state.auth.encryptor);
    match admin
        .create_user(&username, &password, params.admin_role.unwrap_or(false))
        .await
    {
        Ok(user) => {
            let id: i64 = match user.id.parse() {
                Ok(id) => id,
                Err(_) => return response::internal(format),
            };
            match state.index.users().set_email(id, Some(&email)).await {
                Ok(true) => response::empty(format),
                Ok(false) => response::not_found(format),
                Err(error) => {
                    tracing::error!(%error, "createUser 邮箱保存失败");
                    let _ = admin.delete_user(id).await;
                    response::internal(format)
                }
            }
        }
        Err(error) => {
            tracing::error!(%error, "createUser 创建失败");
            response::internal(format)
        }
    }
}

async fn update_user(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    ApiQuery(params): ApiQuery<UpdateParams>,
    ApiAdmin(_admin): ApiAdmin,
) -> Response {
    let format = Format::from_uri(&uri);
    let Some(username) = params.username else {
        return response::parameter_error(format, "Required parameter 'username' is missing");
    };
    let user = match state.index.users().get_user_by_name(&username).await {
        Ok(Some(user)) => user,
        Ok(None) => return response::not_found(format),
        Err(error) => {
            tracing::error!(%error, "updateUser 用户查询失败");
            return response::internal(format);
        }
    };
    let id: i64 = match user.id.parse() {
        Ok(id) => id,
        Err(error) => {
            tracing::error!(%error, "updateUser 用户主键非法");
            return response::internal(format);
        }
    };
    let admin = UserAdmin::new(&state.index, &state.auth.encryptor);
    let effective_admin = params.admin_role.unwrap_or(user.admin);
    if !permissions_supported(
        effective_admin,
        params.settings_role,
        params.stream_role,
        params.download_role,
        params.upload_role,
        params.playlist_role,
        params.cover_art_role,
        params.comment_role,
        params.jukebox_role,
        params.podcast_role,
        params.share_role,
        params.video_conversion_role,
        params.ldap_authenticated,
        params.max_bit_rate,
        params.music_folder_id.as_deref(),
    ) {
        return response::error(format, 0, "Requested user permissions are not supported");
    }
    if let Some(email) = params.email.as_deref() {
        if let Err(error) = state.index.users().set_email(id, Some(email)).await {
            tracing::error!(%error, "updateUser 邮箱更新失败");
            return response::internal(format);
        }
    }
    if let Some(password) = params.password {
        let Some(password) = decode_password(&password) else {
            return response::parameter_error(format, "password is malformed");
        };
        if let Err(error) = admin.change_password(id, &password).await {
            tracing::error!(%error, "updateUser 密码更新失败");
            return response::internal(format);
        }
    }
    if let Some(wants_admin) = params.admin_role {
        let role = match state.index.roles().get_role_by_name("admin").await {
            Ok(Some(role)) => role,
            Ok(None) => {
                tracing::error!("updateUser 缺少内建 admin 角色");
                return response::internal(format);
            }
            Err(error) => {
                tracing::error!(%error, "updateUser 角色查询失败");
                return response::internal(format);
            }
        };
        let role_id: i64 = match role.id.parse() {
            Ok(id) => id,
            Err(error) => {
                tracing::error!(%error, "updateUser 角色主键非法");
                return response::internal(format);
            }
        };
        let result = if wants_admin {
            admin.assign_role(id, role_id).await.map(|()| true)
        } else {
            admin.unassign_role(id, role_id).await
        };
        if let Err(error) = result {
            tracing::error!(%error, "updateUser admin 角色更新失败");
            return response::internal(format);
        }
    }
    response::empty(format)
}

async fn delete_user(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    ApiQuery(params): ApiQuery<UsernameParams>,
    ApiAdmin(_admin): ApiAdmin,
) -> Response {
    let format = Format::from_uri(&uri);
    let Some(username) = params.username else {
        return response::parameter_error(format, "Required parameter 'username' is missing");
    };
    let user = match state.index.users().get_user_by_name(&username).await {
        Ok(Some(user)) => user,
        Ok(None) => return response::not_found(format),
        Err(error) => {
            tracing::error!(%error, "deleteUser 用户查询失败");
            return response::internal(format);
        }
    };
    let Ok(id) = user.id.parse() else {
        return response::internal(format);
    };
    let admin = UserAdmin::new(&state.index, &state.auth.encryptor);
    match admin.delete_user(id).await {
        Ok(true) => response::empty(format),
        Ok(false) => response::not_found(format),
        Err(error) => {
            tracing::error!(%error, "deleteUser 删除失败");
            response::internal(format)
        }
    }
}

async fn change_password(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    ApiQuery(params): ApiQuery<ChangePasswordParams>,
    ApiUser(caller): ApiUser,
) -> Response {
    let format = Format::from_uri(&uri);
    let (Some(username), Some(password)) = (params.username, params.password) else {
        return response::parameter_error(format, "username and password are required");
    };
    if !caller.admin && caller.name != username {
        return response::auth_error(format, crate::auth::AuthError::Forbidden);
    }
    let Some(password) = decode_password(&password) else {
        return response::parameter_error(format, "password is malformed");
    };
    let user = match state.index.users().get_user_by_name(&username).await {
        Ok(Some(user)) => user,
        Ok(None) => return response::not_found(format),
        Err(error) => {
            tracing::error!(%error, "changePassword 用户查询失败");
            return response::internal(format);
        }
    };
    let Ok(id) = user.id.parse() else {
        return response::internal(format);
    };
    let admin = UserAdmin::new(&state.index, &state.auth.encryptor);
    match admin.change_password(id, &password).await {
        Ok(true) => response::empty(format),
        Ok(false) => response::not_found(format),
        Err(error) => {
            tracing::error!(%error, "changePassword 更新失败");
            response::internal(format)
        }
    }
}

fn decode_password(password: &str) -> Option<String> {
    let Some(encoded) = password.strip_prefix("enc:") else {
        return Some(password.to_string());
    };
    String::from_utf8(hex::decode(encoded).ok()?).ok()
}

#[allow(clippy::too_many_arguments)]
fn permissions_supported(
    admin: bool,
    settings: Option<bool>,
    stream: Option<bool>,
    download: Option<bool>,
    upload: Option<bool>,
    playlist: Option<bool>,
    cover_art: Option<bool>,
    comment: Option<bool>,
    jukebox: Option<bool>,
    podcast: Option<bool>,
    share: Option<bool>,
    video_conversion: Option<bool>,
    ldap: Option<bool>,
    max_bitrate: Option<u32>,
    music_folder: Option<&str>,
) -> bool {
    let matches =
        |value: Option<bool>, supported: bool| value.is_none_or(|value| value == supported);
    matches(settings, true)
        && matches(stream, true)
        && matches(download, true)
        && matches(upload, admin)
        && matches(playlist, true)
        && matches(cover_art, true)
        && matches(comment, true)
        && matches(jukebox, false)
        && matches(podcast, false)
        && matches(share, false)
        && matches(video_conversion, false)
        && matches(ldap, false)
        && max_bitrate.is_none_or(|value| value == 0)
        && music_folder.is_none()
}
