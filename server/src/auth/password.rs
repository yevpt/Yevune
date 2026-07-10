//! 密码可逆加密存储（红线）。
//!
//! OpenSubsonic 的 token 认证要求服务端能**还原明文密码**以计算 `md5(密码 + 盐)` 比对，
//! 故密码不能单向哈希，而是以对称加密（AES-256-GCM）存储，同 Navidrome 思路。
//! 密钥由应用密钥（配置项）经 SHA-256 派生，本模块只接收派生用的密钥字符串，不读配置。
//!
//! 存储格式：`base64(nonce(12B) || 密文||tag)`。每次加密用随机 nonce（AES-GCM 要求
//! 同密钥下 nonce 不复用），故同一明文两次加密结果不同。

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use rand::RngCore;
use sha2::{Digest, Sha256};

use super::AuthError;

/// AES-GCM 的 nonce 长度（字节）。
const NONCE_LEN: usize = 12;

/// 密码加解密器（AES-256-GCM）。
#[derive(Clone)]
pub struct Encryptor {
    cipher: Aes256Gcm,
}

impl Encryptor {
    /// 由应用密钥字符串派生 AES-256 密钥并构造加密器。
    ///
    /// 派生：`key = SHA-256(secret)`（32 字节，正好是 AES-256 密钥长度）。
    pub fn new(secret: &str) -> Self {
        let key = Sha256::digest(secret.as_bytes());
        let cipher = Aes256Gcm::new(&key);
        Self { cipher }
    }

    /// 加密明文密码，返回可存库的文本（base64 编码的 `nonce || 密文`）。
    ///
    /// 每次调用使用随机 nonce，故同一明文两次加密结果不同。
    pub fn encrypt(&self, plaintext: &str) -> String {
        let mut nonce_bytes = [0u8; NONCE_LEN];
        rand::rngs::OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        // AES-GCM 加密不会失败（除非分配失败），明文长度无上限约束。
        let ciphertext = self
            .cipher
            .encrypt(nonce, plaintext.as_bytes())
            .expect("AES-GCM 加密失败");
        let mut combined = Vec::with_capacity(NONCE_LEN + ciphertext.len());
        combined.extend_from_slice(&nonce_bytes);
        combined.extend_from_slice(&ciphertext);
        B64.encode(combined)
    }

    /// 解密 [`Encryptor::encrypt`] 产生的文本，还原明文密码。
    ///
    /// 密文损坏、nonce 缺失、密钥不匹配（GCM tag 校验失败）均返回 [`AuthError::Crypto`]。
    pub fn decrypt(&self, enc: &str) -> Result<String, AuthError> {
        let combined = B64.decode(enc).map_err(|_| AuthError::Crypto)?;
        if combined.len() < NONCE_LEN {
            return Err(AuthError::Crypto);
        }
        let (nonce_bytes, ciphertext) = combined.split_at(NONCE_LEN);
        let nonce = Nonce::from_slice(nonce_bytes);
        let plaintext = self
            .cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| AuthError::Crypto)?;
        String::from_utf8(plaintext).map_err(|_| AuthError::Crypto)
    }
}
