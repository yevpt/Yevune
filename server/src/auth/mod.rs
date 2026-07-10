//! 认证与用户/角色管理（设计文档 §10）。
//!
//! 组成：
//! - [`password`]：密码**可逆加密**存储（红线；支持 OpenSubsonic token 校验，同 Navidrome 思路）。
//! - [`subsonic`]：OpenSubsonic 认证（`u`/`t`/`s` token 或明文 `p`，支持纯 HTTP）。
//! - [`bearer`]：自研 Bearer 会话令牌的无状态签发/校验。
//! - [`middleware`]：axum 提取器 [`CurrentUser`]/[`AdminUser`]，把请求解析成用户身份与角色。
//! - [`user_admin`]：用户/角色管理逻辑。
//!
//! 本模块**只暴露 handler/提取器供 T7 注册路由**，不改路由树（并行协调规则）。
//! 授权判定在服务端强制，客户端不可绕过。

pub mod bearer;
pub mod middleware;
pub mod password;
pub mod subsonic;
pub mod user_admin;

pub use bearer::{issue_bearer, issue_bearer_with_expiry, verify_bearer, BearerKey};
pub use middleware::{AdminUser, AuthState, CurrentUser};
pub use password::Encryptor;
pub use subsonic::{verify_subsonic, SubsonicCredentials};
pub use user_admin::UserAdmin;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

/// 认证/授权错误。
///
/// 变体携带足够信息供 T7 映射到 OpenSubsonic 错误信封（[`AuthError::subsonic_code`]）
/// 或直接作为提取器的 HTTP 拒绝响应。
#[derive(Debug)]
pub enum AuthError {
    /// 请求未携带任何可识别凭证。
    MissingCredentials,
    /// 凭证格式非法（如令牌结构损坏、`enc:` 十六进制解不开）。
    MalformedCredentials,
    /// 用户名/密码或令牌不匹配。
    BadCredentials,
    /// 令牌已过期。
    Expired,
    /// 用户不存在。
    UnknownUser,
    /// 已认证但权限不足（非管理员访问管理接口）。
    Forbidden,
    /// 底层存储错误。
    Db(sqlx::Error),
    /// 密码解密/加密失败（密钥不匹配或密文损坏）。
    Crypto,
}

impl AuthError {
    /// 对应的 OpenSubsonic 错误码（供 T7 组装 `subsonic-response` 错误信封）。
    ///
    /// 参考 OpenSubsonic：`10` 参数缺失、`40` 用户名或密码错误、`50` 权限不足、`70` 未找到。
    pub fn subsonic_code(&self) -> u32 {
        match self {
            AuthError::MissingCredentials => 10,
            AuthError::MalformedCredentials => 10,
            AuthError::BadCredentials | AuthError::Expired | AuthError::Crypto => 40,
            AuthError::UnknownUser => 40,
            AuthError::Forbidden => 50,
            AuthError::Db(_) => 0,
        }
    }

    /// 对应的 HTTP 状态码（提取器拒绝时使用）。
    pub fn status(&self) -> StatusCode {
        match self {
            AuthError::Forbidden => StatusCode::FORBIDDEN,
            AuthError::Db(_) => StatusCode::INTERNAL_SERVER_ERROR,
            _ => StatusCode::UNAUTHORIZED,
        }
    }
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthError::MissingCredentials => write!(f, "缺少认证凭证"),
            AuthError::MalformedCredentials => write!(f, "认证凭证格式非法"),
            AuthError::BadCredentials => write!(f, "用户名或密码错误"),
            AuthError::Expired => write!(f, "会话令牌已过期"),
            AuthError::UnknownUser => write!(f, "用户不存在"),
            AuthError::Forbidden => write!(f, "权限不足"),
            AuthError::Db(e) => write!(f, "存储错误：{e}"),
            AuthError::Crypto => write!(f, "密码加解密失败"),
        }
    }
}

impl std::error::Error for AuthError {}

impl From<sqlx::Error> for AuthError {
    fn from(e: sqlx::Error) -> Self {
        AuthError::Db(e)
    }
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        (self.status(), self.to_string()).into_response()
    }
}
