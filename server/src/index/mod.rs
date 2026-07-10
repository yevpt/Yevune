//! 元数据索引层：SQLite 模式、迁移与类型化数据访问（仓储）。
//!
//! 红线：SQLite 索引放服务器**本地磁盘**，开 WAL；运行时查询（非编译期宏），
//! 正确性由集成测试保证。本层不含 HTTP、不含扫描/转码逻辑。

use std::path::Path;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::SqlitePool;

pub mod repo_access;
pub mod repo_annotation;
pub mod repo_media;
pub mod repo_playlist;
pub mod repo_user;

pub use repo_access::{AccessRepo, TrackScope};
pub use repo_annotation::{Annotation, AnnotationRepo};
pub use repo_media::{MediaRepo, NewTrack, SearchResults};
pub use repo_playlist::PlaylistRepo;
pub use repo_user::{RoleRepo, UserRepo};

/// 本层统一结果类型（迁移错误并入 [`sqlx::Error`]）。
pub type Result<T> = std::result::Result<T, sqlx::Error>;

/// 嵌入 `migrations/` 下的迁移脚本。
static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// 索引句柄，持有 SQLite 连接池。
#[derive(Clone)]
pub struct Index {
    pool: SqlitePool,
}

impl Index {
    /// 打开（必要时创建）本地 SQLite 文件，开启 WAL 与外键，并应用全部迁移。
    pub async fn connect(path: &Path) -> Result<Self> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new().connect_with(options).await?;
        MIGRATOR
            .run(&pool)
            .await
            .map_err(|e| sqlx::Error::Migrate(Box::new(e)))?;
        Ok(Self { pool })
    }

    /// 访问底层连接池（供各仓储使用）。
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// 媒体仓储。
    pub fn media(&self) -> MediaRepo<'_> {
        MediaRepo::new(&self.pool)
    }

    /// 用户仓储。
    pub fn users(&self) -> UserRepo<'_> {
        UserRepo::new(&self.pool)
    }

    /// 角色仓储。
    pub fn roles(&self) -> RoleRepo<'_> {
        RoleRepo::new(&self.pool)
    }

    /// 歌单/文件夹仓储。
    pub fn playlists(&self) -> PlaylistRepo<'_> {
        PlaylistRepo::new(&self.pool)
    }

    /// 标注仓储。
    pub fn annotations(&self) -> AnnotationRepo<'_> {
        AnnotationRepo::new(&self.pool)
    }

    /// 访问控制仓储。
    pub fn access(&self) -> AccessRepo<'_> {
        AccessRepo::new(&self.pool)
    }
}
