//! axum 提取器：把请求解析成 [`CurrentUser`]（含角色），并提供仅管理员的 [`AdminUser`]。
//!
//! 解析顺序：先看 `Authorization: Bearer <token>`（自研会话令牌），否则回退到 OpenSubsonic
//! 查询参数 `u`/`t`/`s`/`p`。授权在服务端强制：无有效凭证一律拒绝。
//!
//! 提取器泛型于应用状态 `S`，只要 `AuthState: FromRef<S>` 即可组合，**不绑定具体路由树**
//! （T7 负责把 [`AuthState`] 放进其应用状态）。

use std::collections::HashMap;

use axum::extract::{FromRef, FromRequestParts, Query};
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;

use super::bearer::{verify_bearer, BearerKey};
use super::password::Encryptor;
use super::subsonic::{verify_subsonic, SubsonicCredentials};
use super::AuthError;
use crate::index::Index;

/// 认证所需的共享状态：索引句柄 + 密码加密器 + 令牌签名密钥。
#[derive(Clone)]
pub struct AuthState {
    /// 元数据索引（含 UserRepo/RoleRepo）。
    pub index: Index,
    /// 密码加解密器。
    pub encryptor: Encryptor,
    /// Bearer 令牌签名密钥。
    pub bearer_key: BearerKey,
}

impl AuthState {
    /// 由索引与单一应用密钥构造：密码密钥与令牌密钥各自域分离派生。
    pub fn new(index: Index, app_secret: &str) -> Self {
        Self {
            index,
            encryptor: Encryptor::new(&format!("pwd:{app_secret}")),
            bearer_key: BearerKey::derive(&format!("bearer:{app_secret}")),
        }
    }
}

/// 当前请求的认证用户（含角色与 admin 标记）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentUser {
    /// 用户主键 id。
    pub id: i64,
    /// 用户名。
    pub name: String,
    /// 所属角色名列表。
    pub roles: Vec<String>,
    /// 是否管理员（拥有内建 `admin` 角色）。
    pub admin: bool,
}

impl CurrentUser {
    /// 是否管理员。
    pub fn is_admin(&self) -> bool {
        self.admin
    }
}

#[axum::async_trait]
impl<S> FromRequestParts<S> for CurrentUser
where
    AuthState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AuthError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let auth = AuthState::from_ref(state);

        // 1) 优先 Authorization: Bearer <token>（自研会话令牌）。
        if let Some(value) = parts.headers.get(AUTHORIZATION) {
            if let Ok(text) = value.to_str() {
                if let Some(token) = text.strip_prefix("Bearer ") {
                    let id = verify_bearer(&auth.bearer_key, token.trim())?;
                    return load_current_user(&auth.index, id).await;
                }
            }
        }

        // 2) 回退到 OpenSubsonic 查询参数 u/t/s/p。
        let params: HashMap<String, String> = Query::from_request_parts(parts, state)
            .await
            .map(|Query(m)| m)
            .unwrap_or_default();
        if let Some(username) = params.get("u") {
            let creds = SubsonicCredentials {
                username: username.clone(),
                token: params.get("t").cloned(),
                salt: params.get("s").cloned(),
                password: params.get("p").cloned(),
            };
            let id = verify_subsonic(&auth.index.users(), &auth.encryptor, &creds).await?;
            return load_current_user(&auth.index, id).await;
        }

        Err(AuthError::MissingCredentials)
    }
}

/// 按用户 id 从索引装载 [`CurrentUser`]（含角色与 admin 标记）。
async fn load_current_user(index: &Index, id: i64) -> Result<CurrentUser, AuthError> {
    let user = index
        .users()
        .get_user(id)
        .await?
        .ok_or(AuthError::UnknownUser)?;
    Ok(CurrentUser {
        id,
        name: user.name,
        roles: user.roles,
        admin: user.admin,
    })
}

/// 仅管理员可提取；非管理员返回 [`AuthError::Forbidden`]。
#[derive(Debug, Clone)]
pub struct AdminUser(pub CurrentUser);

#[axum::async_trait]
impl<S> FromRequestParts<S> for AdminUser
where
    AuthState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AuthError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let user = CurrentUser::from_request_parts(parts, state).await?;
        if user.admin {
            Ok(AdminUser(user))
        } else {
            Err(AuthError::Forbidden)
        }
    }
}
