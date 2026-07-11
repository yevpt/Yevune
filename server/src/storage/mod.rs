//! 对象存储层：读写 Garage（S3 兼容）的窄抽象。
//!
//! 对外只暴露自研的 [`ObjectStore`] trait
//!（`list`/`get`/`get_range`/`put`/`put_file`/`delete`/`head`），
//! 不泄漏底层 `object_store` 类型给 scanner/transcode，保留将来换实现的自由（见 [ADR-0005]）。
//! trait 可用内存假实现 [`MemoryStore`] 替换以便单测，Garage 实现见 [`GarageStore`]。
//!
//! 本层只做存储原语，**不含**扫描/转码/HTTP。
//!
//! [ADR-0005]: ../../../docs/adr/0005-object-store-over-aws-sdk.md

use std::fmt;
use std::ops::Range;
use std::path::Path;

use async_trait::async_trait;
use bytes::Bytes;

pub mod garage;
pub mod memory;

pub use garage::{GarageConfig, GarageStore};
pub use memory::MemoryStore;

/// 分页列举的默认页大小（对齐 S3 单次 List 上限）。
pub const DEFAULT_PAGE_SIZE: usize = 1000;

/// 文件读写的固定分块大小；上传与转码全程以此建立有界缓冲。
pub const STREAM_CHUNK_SIZE: usize = 64 * 1024;

/// 本层统一结果类型。
pub type Result<T> = std::result::Result<T, StorageError>;

/// 存储层错误。
#[derive(Debug)]
pub enum StorageError {
    /// 目标 key 不存在（供 scanner 区分"已删除"）。
    NotFound(String),
    /// 后端错误（网络/权限/协议等），携带可读描述。
    Backend(String),
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StorageError::NotFound(key) => write!(f, "对象不存在: {key}"),
            StorageError::Backend(msg) => write!(f, "存储后端错误: {msg}"),
        }
    }
}

impl std::error::Error for StorageError {}

/// 对象元数据（`head`/`put` 结果）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectMeta {
    /// 对象的 ETag（变更检测用），部分后端可能缺省。
    pub etag: Option<String>,
    /// 对象字节大小。
    pub size: u64,
}

/// 列举结果中的一项。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListEntry {
    /// 对象 key（完整路径）。
    pub key: String,
    /// 对象的 ETag。
    pub etag: Option<String>,
    /// 对象字节大小。
    pub size: u64,
}

/// 一页列举结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListPage {
    /// 本页条目，按 key 升序。
    pub entries: Vec<ListEntry>,
    /// 续页游标（不透明，等于本页末尾 key 的 start-after 语义）；
    /// 为 `None` 表示已到末页。
    pub next_token: Option<String>,
}

/// 对象存储抽象。实现方需保证跨页列举 **完整、有序、不重不漏**。
#[async_trait]
pub trait ObjectStore: Send + Sync {
    /// 按 `prefix` 分页列举对象。
    ///
    /// `token` 为上一页返回的 [`ListPage::next_token`]（首页传 `None`）。
    /// 返回条目按 key 升序；当仍有后续对象时 `next_token` 为 `Some`。
    async fn list(&self, prefix: &str, token: Option<String>) -> Result<ListPage>;

    /// 读取整个对象。大对象请改用 [`get_range`](ObjectStore::get_range) 避免整读进内存。
    async fn get(&self, key: &str) -> Result<Bytes>;

    /// 读取对象的半开字节区间 `[start, end)`。
    async fn get_range(&self, key: &str, range: Range<u64>) -> Result<Bytes>;

    /// 写入对象，返回其元数据（含 ETag）。
    async fn put(&self, key: &str, bytes: Bytes) -> Result<ObjectMeta>;

    /// 从本地文件有界分块上传对象，避免将完整文件组装为单个 [`Bytes`]。
    ///
    /// 实现必须只在全部分块成功后使对象可见；失败时需中止 multipart 并清理残留。
    async fn put_file(&self, key: &str, path: &Path) -> Result<ObjectMeta>;

    /// 删除对象。不存在时视为成功（幂等，对齐 S3 语义）。
    async fn delete(&self, key: &str) -> Result<()>;

    /// 读取对象元数据（ETag + size），不下载内容。key 不存在时返回 [`StorageError::NotFound`]。
    async fn head(&self, key: &str) -> Result<ObjectMeta>;
}
