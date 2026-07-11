//! 首启引导：无用户时创建管理员（设计文档 §11）。
//!
//! 幂等：仅当索引中尚无任何用户时创建管理员，已初始化则跳过。密码来自配置/环境；
//! 未提供时生成一次性随机密码并回传给调用方展示（小白友好，随后应尽快改密）。

use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64URL;
use base64::Engine;
use rand::RngCore;

use crate::auth::{AuthError, Encryptor, UserAdmin};
use crate::index::Index;

/// 首启管理员的账号信息。
#[derive(Debug, Clone)]
pub struct AdminSeed {
    /// 管理员用户名。
    pub username: String,
    /// 管理员密码；`None` 表示由服务端生成随机密码。
    pub password: Option<String>,
}

/// 首启引导结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetupOutcome {
    /// 索引中已有用户，未做任何改动。
    AlreadyInitialized,
    /// 新建了管理员账号。随机生成密码时 `generated_password` 给出明文（仅此一次可见）。
    AdminCreated {
        /// 管理员用户名。
        username: String,
        /// 服务端生成的随机密码（仅在未提供密码时为 `Some`）。
        generated_password: Option<String>,
    },
}

/// 若索引中尚无任何用户，则按 `seed` 创建管理员；否则跳过。
///
/// `app_secret` 须与运行时 [`crate::auth::AuthState`] 使用的一致（`pwd:{app_secret}` 派生
/// 加密器），否则新建管理员将无法通过认证。返回 [`SetupOutcome`] 供上层记录/展示。
pub async fn ensure_admin(
    index: &Index,
    app_secret: &str,
    seed: &AdminSeed,
) -> Result<SetupOutcome, AuthError> {
    if !index.users().list_users().await?.is_empty() {
        return Ok(SetupOutcome::AlreadyInitialized);
    }
    let (password, generated) = match seed.password.as_deref() {
        Some(pw) if !pw.is_empty() => (pw.to_string(), None),
        _ => {
            let pw = generate_password();
            (pw.clone(), Some(pw))
        }
    };
    let encryptor = Encryptor::new(&format!("pwd:{app_secret}"));
    let admin = UserAdmin::new(index, &encryptor);
    admin.create_user(&seed.username, &password, true).await?;
    Ok(SetupOutcome::AdminCreated {
        username: seed.username.clone(),
        generated_password: generated,
    })
}

/// 生成一次性随机密码（128 bit 熵，URL-safe base64，无填充，长度 22）。
fn generate_password() -> String {
    let mut bytes = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    B64URL.encode(bytes)
}
