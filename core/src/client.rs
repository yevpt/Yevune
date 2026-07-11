//! UniFFI 暴露的跨平台客户端门面。

use std::sync::Arc;

use tokio::sync::RwLock;

use crate::api::browse::{self, AlbumDetail, AlbumSort, ArtistDetail, SearchResult};
use crate::api::manage::{self, TagUpdate, UploadMetadata, UploadProgress};
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
        let session = self.authenticated_session().await?;
        self.http.get_empty(&session, "ping").await
    }

    /// 读取一页专辑。
    pub async fn list_albums(
        &self,
        sort: AlbumSort,
        offset: u32,
        size: u32,
    ) -> Result<Vec<contract::Album>> {
        browse::list_albums(
            &self.http,
            &self.authenticated_session().await?,
            sort,
            offset,
            size,
        )
        .await
    }

    /// 读取专辑及其曲目。
    pub async fn get_album(&self, id: String) -> Result<AlbumDetail> {
        browse::get_album(&self.http, &self.authenticated_session().await?, id).await
    }

    /// 读取艺人及其专辑。
    pub async fn get_artist(&self, id: String) -> Result<ArtistDetail> {
        browse::get_artist(&self.http, &self.authenticated_session().await?, id).await
    }

    /// 读取单曲。
    pub async fn get_song(&self, id: String) -> Result<contract::Track> {
        browse::get_song(&self.http, &self.authenticated_session().await?, id).await
    }

    /// 读取所有可见艺人。
    pub async fn list_artists(&self) -> Result<Vec<contract::Artist>> {
        browse::list_artists(&self.http, &self.authenticated_session().await?).await
    }

    /// 在艺人、专辑与曲目中全文搜索。
    pub async fn search(&self, query: String) -> Result<SearchResult> {
        browse::search(&self.http, &self.authenticated_session().await?, query).await
    }

    /// 从本地路径流式上传单曲；音频字节不会穿越 FFI。
    pub async fn upload_track(
        &self,
        local_path: String,
        metadata: UploadMetadata,
        progress: Box<dyn UploadProgress>,
    ) -> Result<contract::Track> {
        manage::upload_track(
            &self.http,
            &self.authenticated_session().await?,
            local_path,
            metadata,
            progress,
        )
        .await
    }

    /// 写入服务端标签覆盖层，不修改原始音频文件。
    pub async fn update_tags(&self, id: String, update: TagUpdate) -> Result<()> {
        manage::update_tags(&self.http, &self.authenticated_session().await?, id, update).await
    }
}

impl MusicClient {
    async fn authenticated_session(&self) -> Result<AuthenticatedSession> {
        self.session
            .read()
            .await
            .clone()
            .ok_or(CoreError::NotAuthenticated)
    }
}
