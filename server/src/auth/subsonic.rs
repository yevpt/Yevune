//! OpenSubsonic 认证：token（`u`/`t`/`s`）或明文（`p`）。
//!
//! 纯 HTTP 下优先 token 认证（`t = md5(密码 + 盐)`）避免裸传密码；明文 `p` 也支持，
//! 其可为 `enc:<hex>` 形式（十六进制编码）。校验需还原明文密码（见 [`super::password`]）。

use md5::{Digest, Md5};

use super::password::Encryptor;
use super::AuthError;
use crate::index::UserRepo;

/// 一次请求携带的 OpenSubsonic 凭证。
#[derive(Debug, Clone, Default)]
pub struct SubsonicCredentials {
    /// 用户名 `u`。
    pub username: String,
    /// token `t`（= `md5(密码 + 盐)` 的十六进制）。
    pub token: Option<String>,
    /// 盐 `s`。
    pub salt: Option<String>,
    /// 明文密码 `p`（可为 `enc:<hex>`）。
    pub password: Option<String>,
}

/// 校验一组 OpenSubsonic 凭证，成功返回用户主键 id。
///
/// 优先走 token 路径（`t`+`s`），否则走明文 `p`；两者皆无则 [`AuthError::MissingCredentials`]。
pub async fn verify_subsonic(
    users: &UserRepo<'_>,
    enc: &Encryptor,
    creds: &SubsonicCredentials,
) -> Result<i64, AuthError> {
    // 先取存储的密文密码；用户不存在则拒。
    let Some(password_enc) = users.password_enc(&creds.username).await? else {
        return Err(AuthError::UnknownUser);
    };
    let stored = enc.decrypt(&password_enc)?;

    // 校验密码：token 路径优先，其次明文 p。
    let ok = match (&creds.token, &creds.salt, &creds.password) {
        (Some(token), Some(salt), _) => {
            let expected = subsonic_token(&stored, salt);
            constant_time_eq(expected.as_bytes(), token.to_ascii_lowercase().as_bytes())
        }
        (_, _, Some(p)) => {
            let candidate = decode_password_param(p)?;
            constant_time_eq(candidate.as_bytes(), stored.as_bytes())
        }
        _ => return Err(AuthError::MissingCredentials),
    };
    if !ok {
        return Err(AuthError::BadCredentials);
    }

    // 密码正确后取用户 id。
    let user = users
        .get_user_by_name(&creds.username)
        .await?
        .ok_or(AuthError::UnknownUser)?;
    user.id.parse::<i64>().map_err(|_| AuthError::UnknownUser)
}

/// 计算 OpenSubsonic token = hex(md5(密码 + 盐))（小写）。
fn subsonic_token(password: &str, salt: &str) -> String {
    let mut h = Md5::new();
    h.update(password.as_bytes());
    h.update(salt.as_bytes());
    hex::encode(h.finalize())
}

/// 解析明文密码参数 `p`：`enc:<hex>` 先十六进制解码，否则原样返回。
fn decode_password_param(p: &str) -> Result<String, AuthError> {
    if let Some(rest) = p.strip_prefix("enc:") {
        let bytes = hex::decode(rest).map_err(|_| AuthError::MalformedCredentials)?;
        String::from_utf8(bytes).map_err(|_| AuthError::MalformedCredentials)
    } else {
        Ok(p.to_string())
    }
}

/// 定长常量时间比较，避免密码/令牌校验的时序侧信道。
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}
