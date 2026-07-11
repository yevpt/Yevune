//! UniFFI 暴露的跨平台客户端门面。

use std::sync::Arc;

use tokio::sync::RwLock;

use crate::auth::AuthenticatedSession;
use crate::config::ServerConfig;
use crate::error::{CoreError, Result};
use crate::http::HttpClient;

/// 不含密码的已登录会话信息。
#[derive(Debug, Clone, uniffi::Record)]
pub struct Session {
    /// 规范化后的服务器根地址。
    pub server: String,
    /// 当前用户名。
    pub user: String,
}

/// 所有平台共用的音乐服务客户端。
#[derive(uniffi::Object)]
pub struct MusicClient {
    http: HttpClient,
    session: RwLock<Option<AuthenticatedSession>>,
}

#[uniffi::export(async_runtime = "tokio")]
impl MusicClient {
    /// 创建尚未登录的客户端。
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            http: HttpClient::new(),
            session: RwLock::new(None),
        })
    }

    /// 验证凭证并仅在 ping 成功后保存会话。
    pub async fn login(&self, server: String, user: String, password: String) -> Result<Session> {
        let candidate = AuthenticatedSession {
            config: ServerConfig::parse(&server)?,
            user,
            password,
        };
        self.http.get_empty(&candidate, "ping").await?;
        let session = Session {
            server: candidate.config.public_url(),
            user: candidate.user.clone(),
        };
        *self.session.write().await = Some(candidate);
        Ok(session)
    }

    /// 验证当前会话仍可访问服务端。
    pub async fn ping(&self) -> Result<()> {
        let session = self
            .session
            .read()
            .await
            .clone()
            .ok_or(CoreError::NotAuthenticated)?;
        self.http.get_empty(&session, "ping").await
    }
}
