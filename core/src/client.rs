//! UniFFI 暴露的跨平台客户端门面。

use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

use tokio::sync::RwLock;

use crate::api::browse::{
    self, AlbumDetail, AlbumFilter, ArtistDetail, SearchPage, SearchPageRequest, SearchResult,
};
use crate::api::lyrics;
use crate::api::manage::{self, TagUpdate, UploadMetadata, UploadProgress};
use crate::api::media;
use crate::api::playlist::{self, PlaylistDetail, PlaylistTree};
use crate::api::scan::DetailedScanResult;
use crate::api::scan::{self, ScanStatus};
use crate::api::{access, admin};
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
    /// 当前用户是否拥有管理员角色。
    pub admin: bool,
}

/// 所有平台共用的音乐服务客户端。
#[derive(uniffi::Object)]
pub struct MusicClient {
    http: HttpClient,
    session: RwLock<Option<AuthenticatedSession>>,
    session_generation: AtomicU64,
}

#[uniffi::export(async_runtime = "tokio")]
impl MusicClient {
    /// 创建尚未登录的客户端。
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            http: HttpClient::new(),
            session: RwLock::new(None),
            session_generation: AtomicU64::new(0),
        })
    }

    /// 验证凭证并仅在 ping 成功后保存会话。
    pub async fn login(&self, server: String, user: String, password: String) -> Result<Session> {
        let generation = self.session_generation.load(Ordering::Acquire);
        let candidate = AuthenticatedSession {
            config: ServerConfig::parse(&server)?,
            user,
            password,
        };
        self.http.get_empty(&candidate, "ping").await?;
        let admin = admin::current_user_is_admin(&self.http, &candidate).await?;
        let session = Session {
            server: candidate.config.public_url(),
            user: candidate.user.clone(),
            admin,
        };
        let mut current = self.session.write().await;
        if generation != self.session_generation.load(Ordering::Acquire) {
            return Err(CoreError::NotAuthenticated);
        }
        *current = Some(candidate);
        Ok(session)
    }

    /// 清除当前认证会话及其内存中的明文密码。
    pub async fn logout(&self) {
        self.session_generation.fetch_add(1, Ordering::AcqRel);
        *self.session.write().await = None;
    }

    /// 验证当前会话仍可访问服务端。
    pub async fn ping(&self) -> Result<()> {
        let session = self.authenticated_session().await?;
        self.http.get_empty(&session, "ping").await
    }

    /// 读取管理员可管理的完整用户列表。
    pub async fn list_users(&self) -> Result<Vec<contract::User>> {
        admin::list_users(&self.http, &self.authenticated_session().await?).await
    }

    /// 读取内建与自定义角色。
    pub async fn list_roles(&self) -> Result<Vec<contract::Role>> {
        admin::list_roles(&self.http, &self.authenticated_session().await?).await
    }

    /// 创建家庭用户并赋予内建 member 或 admin 角色。
    pub async fn create_user(
        &self,
        username: String,
        email: String,
        password: String,
        admin: bool,
    ) -> Result<()> {
        admin::create_user(
            &self.http,
            &self.authenticated_session().await?,
            username,
            email,
            password,
            admin,
        )
        .await
    }

    /// 更新用户邮箱与管理员状态。
    pub async fn update_user(&self, username: String, email: String, admin: bool) -> Result<()> {
        admin::update_user(
            &self.http,
            &self.authenticated_session().await?,
            username,
            email,
            admin,
        )
        .await
    }

    /// 重置指定用户的密码。
    pub async fn change_password(&self, username: String, password: String) -> Result<()> {
        let session = self.authenticated_session().await?;
        let changes_current_session = username == session.user;
        admin::change_password(&self.http, &session, username, password.clone()).await?;

        if changes_current_session {
            let mut current = self.session.write().await;
            if let Some(current) = current.as_mut() {
                if current.user == session.user && current.password == session.password {
                    current.password = password;
                }
            }
        }
        Ok(())
    }

    /// 删除指定用户。
    pub async fn delete_user(&self, username: String) -> Result<()> {
        admin::delete_user(&self.http, &self.authenticated_session().await?, username).await
    }

    /// 创建自定义角色。
    pub async fn create_role(&self, name: String) -> Result<contract::Role> {
        admin::create_role(&self.http, &self.authenticated_session().await?, name).await
    }

    /// 删除自定义角色；内建角色由服务端拒绝。
    pub async fn delete_role(&self, id: String) -> Result<()> {
        admin::delete_role(&self.http, &self.authenticated_session().await?, id).await
    }

    /// 给用户分配角色。
    pub async fn assign_role(&self, user_id: String, role_id: String) -> Result<()> {
        admin::assign_role(
            &self.http,
            &self.authenticated_session().await?,
            user_id,
            role_id,
        )
        .await
    }

    /// 解除用户的角色。
    pub async fn unassign_role(&self, user_id: String, role_id: String) -> Result<()> {
        admin::unassign_role(
            &self.http,
            &self.authenticated_session().await?,
            user_id,
            role_id,
        )
        .await
    }

    /// 读取全部曲库访问规则。
    pub async fn list_access_rules(&self) -> Result<Vec<contract::AccessRule>> {
        access::list_access_rules(&self.http, &self.authenticated_session().await?).await
    }

    /// 创建或替换指定作用域的访问规则。
    pub async fn set_access_rule(
        &self,
        scope_type: contract::ScopeType,
        scope_id: String,
        grants: Vec<contract::Principal>,
    ) -> Result<contract::AccessRule> {
        access::set_access_rule(
            &self.http,
            &self.authenticated_session().await?,
            scope_type,
            scope_id,
            grants,
        )
        .await
    }

    /// 删除指定访问规则。
    pub async fn delete_access_rule(&self, id: String) -> Result<()> {
        access::delete_access_rule(&self.http, &self.authenticated_session().await?, id).await
    }

    /// 读取一页专辑，按排序/流派/年份区间三态互斥筛选。
    pub async fn list_albums(
        &self,
        filter: AlbumFilter,
        offset: u32,
        size: u32,
    ) -> Result<Vec<contract::Album>> {
        browse::list_albums(
            &self.http,
            &self.authenticated_session().await?,
            filter,
            offset,
            size,
        )
        .await
    }

    /// 读取所有可见流派。
    pub async fn list_genres(&self) -> Result<Vec<contract::Genre>> {
        browse::list_genres(&self.http, &self.authenticated_session().await?).await
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

    /// 读取 OpenSubsonic 结构化歌词；无歌词时返回空列表。
    pub async fn get_lyrics_by_song_id(
        &self,
        id: String,
    ) -> Result<Vec<contract::StructuredLyrics>> {
        lyrics::get_lyrics_by_song_id(&self.http, &self.authenticated_session().await?, id).await
    }

    /// 读取所有可见艺人。
    pub async fn list_artists(&self) -> Result<Vec<contract::Artist>> {
        browse::list_artists(&self.http, &self.authenticated_session().await?).await
    }

    /// 在艺人、专辑与曲目中全文搜索。
    pub async fn search(&self, query: String) -> Result<SearchResult> {
        browse::search(&self.http, &self.authenticated_session().await?, query).await
    }

    /// 在艺人、专辑与曲目中按类型独立分页搜索。
    pub async fn search_page(&self, request: SearchPageRequest) -> Result<SearchPage> {
        for count in [
            request.artist_count,
            request.album_count,
            request.track_count,
        ] {
            if count > 100 {
                return Err(CoreError::InvalidRequest {
                    message: "search count must be <= 100".into(),
                });
            }
        }
        browse::search_page(&self.http, &self.authenticated_session().await?, request).await
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

    /// 删除曲目及其对象存储文件。
    pub async fn delete_track(&self, id: String) -> Result<()> {
        manage::delete_track(&self.http, &self.authenticated_session().await?, id).await
    }

    /// 移动曲目到新的 `library/` 对象键。
    pub async fn move_track(&self, id: String, key: String) -> Result<()> {
        manage::move_track(&self.http, &self.authenticated_session().await?, id, key).await
    }
    pub async fn start_scan(&self) -> Result<ScanStatus> {
        scan::start(&self.http, &self.authenticated_session().await?).await
    }
    pub async fn scan_status(&self) -> Result<ScanStatus> {
        scan::status(&self.http, &self.authenticated_session().await?).await
    }

    /// 同步扫描指定对象键前缀并返回详细变更。
    pub async fn scan_prefix(&self, prefix: String) -> Result<DetailedScanResult> {
        scan::prefix(&self.http, &self.authenticated_session().await?, prefix).await
    }

    /// 生成带当前认证参数的封面 URL。
    pub async fn cover_art_url(&self, id: String, size: Option<u32>) -> Result<String> {
        media::cover_art_url(&self.authenticated_session().await?, id, size)
    }

    /// 从本地路径流式替换专辑封面。
    pub async fn set_cover_art(&self, album_id: String, local_path: String) -> Result<()> {
        media::set_cover_art(&self.authenticated_session().await?, album_id, local_path).await
    }

    /// 生成交给平台播放器的认证流媒体 URL。
    pub async fn stream_url(&self, track_id: String) -> Result<String> {
        media::stream_url(&self.authenticated_session().await?, track_id)
    }

    /// 读取当前用户的歌单文件夹树与叶子歌单。
    pub async fn playlist_tree(&self) -> Result<PlaylistTree> {
        playlist::playlist_tree(&self.http, &self.authenticated_session().await?).await
    }

    /// 读取单个歌单及其曲目。
    pub async fn playlist_detail(&self, id: String) -> Result<PlaylistDetail> {
        playlist::playlist_detail(&self.http, &self.authenticated_session().await?, id).await
    }

    /// 创建歌单；`folder_id` 非空时创建后移动到该文件夹。
    pub async fn create_playlist(
        &self,
        name: String,
        folder_id: Option<String>,
        song_ids: Vec<String>,
    ) -> Result<contract::Playlist> {
        playlist::create_playlist(
            &self.http,
            &self.authenticated_session().await?,
            name,
            folder_id,
            song_ids,
        )
        .await
    }

    /// 把歌单移动到指定文件夹；`folder_id` 为 `None` 表示移到根。
    pub async fn move_playlist(&self, id: String, folder_id: Option<String>) -> Result<()> {
        playlist::move_playlist(
            &self.http,
            &self.authenticated_session().await?,
            id,
            folder_id,
        )
        .await
    }

    /// 删除歌单。
    pub async fn delete_playlist(&self, id: String) -> Result<()> {
        playlist::delete_playlist(&self.http, &self.authenticated_session().await?, id).await
    }

    /// 重命名歌单。
    pub async fn rename_playlist(&self, id: String, name: String) -> Result<()> {
        playlist::rename_playlist(&self.http, &self.authenticated_session().await?, id, name).await
    }

    /// 设置歌单备注。
    pub async fn set_playlist_comment(&self, id: String, comment: String) -> Result<()> {
        playlist::set_playlist_comment(
            &self.http,
            &self.authenticated_session().await?,
            id,
            comment,
        )
        .await
    }

    /// 向歌单追加曲目。
    pub async fn add_tracks(&self, id: String, song_ids: Vec<String>) -> Result<()> {
        playlist::add_tracks(
            &self.http,
            &self.authenticated_session().await?,
            id,
            song_ids,
        )
        .await
    }

    /// 按索引移除歌单中的一条曲目。
    pub async fn remove_track_at(&self, id: String, index: i64) -> Result<()> {
        playlist::remove_track_at(&self.http, &self.authenticated_session().await?, id, index).await
    }

    /// 创建歌单文件夹；`parent_id` 非空时挂到该父文件夹下。
    pub async fn create_folder(
        &self,
        name: String,
        parent_id: Option<String>,
    ) -> Result<contract::PlaylistFolder> {
        playlist::create_folder(
            &self.http,
            &self.authenticated_session().await?,
            name,
            parent_id,
        )
        .await
    }

    /// 重命名歌单文件夹。
    pub async fn rename_folder(&self, id: String, name: String) -> Result<()> {
        playlist::rename_folder(&self.http, &self.authenticated_session().await?, id, name).await
    }

    /// 删除歌单文件夹（服务端会一并移除其内歌单）。
    pub async fn delete_folder(&self, id: String) -> Result<()> {
        playlist::delete_folder(&self.http, &self.authenticated_session().await?, id).await
    }

    /// 把文件夹移动到新父文件夹；`parent_id` 为 `None` 表示移到根。服务端拒绝成环。
    pub async fn move_folder(&self, id: String, parent_id: Option<String>) -> Result<()> {
        playlist::move_folder(
            &self.http,
            &self.authenticated_session().await?,
            id,
            parent_id,
        )
        .await
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
