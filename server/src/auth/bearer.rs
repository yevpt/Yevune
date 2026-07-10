//! 自研 Bearer 会话令牌：无状态、HMAC-SHA256 签名。
//!
//! 令牌自带用户 id 与过期时间并由服务端密钥签名，校验时验签 + 查过期，**无需会话表/Redis**
//! （红线：禁止引入独立缓存/数据库）。格式：`v1.<payload_b64url>.<sig_b64url>`，
//! 其中 `payload = "<user_id>:<exp_unix>"`，`sig = HMAC-SHA256(key, "v1.<payload_b64url>")`。

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64URL;
use base64::Engine;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

use super::AuthError;

type HmacSha256 = Hmac<Sha256>;

/// 令牌版本前缀（便于未来平滑升级签名方案）。
const VERSION: &str = "v1";

/// Bearer 令牌签名密钥（由应用密钥派生，独立于密码加密密钥）。
#[derive(Clone)]
pub struct BearerKey {
    key: [u8; 32],
}

impl BearerKey {
    /// 由应用密钥字符串派生签名密钥（`SHA-256(secret)`）。
    pub fn derive(secret: &str) -> Self {
        let digest = Sha256::digest(secret.as_bytes());
        let mut key = [0u8; 32];
        key.copy_from_slice(&digest);
        Self { key }
    }

    /// 对给定字节串计算 HMAC-SHA256 标签。
    fn sign(&self, message: &[u8]) -> Vec<u8> {
        let mut mac = HmacSha256::new_from_slice(&self.key).expect("HMAC 接受任意长度密钥");
        mac.update(message);
        mac.finalize().into_bytes().to_vec()
    }
}

/// 以给定存活时长签发令牌（过期时间 = 当前时间 + `ttl`）。
pub fn issue_bearer(key: &BearerKey, user_id: i64, ttl: Duration) -> String {
    let exp = now_unix().saturating_add(ttl.as_secs() as i64);
    issue_bearer_with_expiry(key, user_id, exp)
}

/// 以显式过期时间戳（Unix 秒）签发令牌（供测试构造已过期令牌）。
pub fn issue_bearer_with_expiry(key: &BearerKey, user_id: i64, exp_unix: i64) -> String {
    let payload = format!("{user_id}:{exp_unix}");
    let payload_b64 = B64URL.encode(payload.as_bytes());
    let signing_input = format!("{VERSION}.{payload_b64}");
    let sig = B64URL.encode(key.sign(signing_input.as_bytes()));
    format!("{signing_input}.{sig}")
}

/// 校验令牌，成功返回用户主键 id。
///
/// 验签失败 → [`AuthError::BadCredentials`]；结构损坏 → [`AuthError::MalformedCredentials`]；
/// 已过期 → [`AuthError::Expired`]。
pub fn verify_bearer(key: &BearerKey, token: &str) -> Result<i64, AuthError> {
    let mut parts = token.splitn(3, '.');
    let version = parts.next().ok_or(AuthError::MalformedCredentials)?;
    let payload_b64 = parts.next().ok_or(AuthError::MalformedCredentials)?;
    let sig_b64 = parts.next().ok_or(AuthError::MalformedCredentials)?;
    if version != VERSION {
        return Err(AuthError::MalformedCredentials);
    }

    // 先验签（HMAC 的常量时间校验），再解析内容。
    let signing_input = format!("{version}.{payload_b64}");
    let provided = B64URL
        .decode(sig_b64)
        .map_err(|_| AuthError::MalformedCredentials)?;
    let mut mac = HmacSha256::new_from_slice(&key.key).expect("HMAC 接受任意长度密钥");
    mac.update(signing_input.as_bytes());
    if mac.verify_slice(&provided).is_err() {
        return Err(AuthError::BadCredentials);
    }

    // 解析 payload：user_id:exp。
    let payload_bytes = B64URL
        .decode(payload_b64)
        .map_err(|_| AuthError::MalformedCredentials)?;
    let payload = String::from_utf8(payload_bytes).map_err(|_| AuthError::MalformedCredentials)?;
    let (id_str, exp_str) = payload
        .split_once(':')
        .ok_or(AuthError::MalformedCredentials)?;
    let user_id: i64 = id_str
        .parse()
        .map_err(|_| AuthError::MalformedCredentials)?;
    let exp: i64 = exp_str
        .parse()
        .map_err(|_| AuthError::MalformedCredentials)?;

    if now_unix() >= exp {
        return Err(AuthError::Expired);
    }
    Ok(user_id)
}

/// 当前 Unix 时间（秒）。
fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
