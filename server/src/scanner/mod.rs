//! 扫描/入库层：从 Garage 增量入库音频。
//!
//! 流程（设计文档 §7）：列举 bucket → 用 `(key, etag, size)` 比对 `tracks` 表 →
//! 新增/etag 变化/已删除 三类处理；新增/变化者用 [`ObjectStore::get_range`] **只读文件头**
//! 解析标签（红线：绝不整读音频），抽取内嵌封面单独 `put` 回 Garage 记 `cover_key`；
//! 更新 `scan_state`（游标 + 完成时间）记录断点；因入库幂等（按 object_key upsert +
//! etag 跳过），中断后重扫安全。用 tokio 信号量限流并发头部解析。
//!
//! 本层只做入库逻辑，**不含** HTTP 路由——暴露 [`Scanner::scan`] 与 [`Scanner::scan_status`]
//! 供后续 `startScan`/`getScanStatus` 调用。

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use tokio::sync::Semaphore;

use crate::index::Index;
use crate::storage::{ListEntry, ObjectStore, StorageError};

mod cover;
mod incremental;
pub(crate) mod lyrics;
pub(crate) mod tags;

/// 可入库的音频扩展名（小写，不含点）。非此列表的对象（如封面）扫描时跳过。
const AUDIO_EXTS: &[&str] = &[
    "flac", "mp3", "m4a", "mp4", "aac", "ogg", "oga", "opus", "wav", "wma", "ape", "wv",
];

/// 头部读取上限（字节）：只读文件头解析元数据 + 内嵌封面，绝不整读音频（红线）。
///
/// FLAC 的 STREAMINFO/VORBIS_COMMENT/PICTURE 元数据块位于音频帧之前，1 MiB 足以覆盖
/// 常规标签与专辑封面；超出者封面可能截断（可接受），标签仍可解析。
pub const HEADER_READ_CAP: u64 = 1024 * 1024;

/// 默认并发头部解析上限。
pub const DEFAULT_CONCURRENCY: usize = 4;

/// 本层统一结果类型。
pub type Result<T> = std::result::Result<T, Error>;

/// 扫描层错误。
#[derive(Debug)]
pub enum Error {
    /// 已有扫描正在运行。
    AlreadyScanning,
    /// 存储后端错误。
    Storage(StorageError),
    /// 索引/数据库错误。
    Db(sqlx::Error),
    /// 标签解析错误（携带 object_key 与原因）。
    Parse(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::AlreadyScanning => write!(f, "扫描已在运行"),
            Error::Storage(e) => write!(f, "存储错误: {e}"),
            Error::Db(e) => write!(f, "索引错误: {e}"),
            Error::Parse(msg) => write!(f, "标签解析失败: {msg}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<StorageError> for Error {
    fn from(e: StorageError) -> Self {
        Error::Storage(e)
    }
}

impl From<sqlx::Error> for Error {
    fn from(e: sqlx::Error) -> Self {
        Error::Db(e)
    }
}

/// 一次扫描的结果统计。
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ScanReport {
    /// 新增入库的曲目数。
    pub added: u32,
    /// etag 变化后更新的曲目数。
    pub updated: u32,
    /// 因源文件消失而删除的曲目数。
    pub deleted: u32,
    /// etag 未变、跳过的曲目数。
    pub unchanged: u32,
    /// 本轮变更明细（最多 500 条）。
    pub changes: Vec<ScanChange>,
    /// 变更总数是否超过明细上限。
    pub changes_truncated: bool,
}

/// 一条扫描变更，用于原生客户端可视化反馈。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanChange {
    pub action: ScanAction,
    pub object_key: String,
    pub track: contract::Track,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanAction {
    Added,
    Updated,
    Deleted,
}

impl ScanAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Updated => "updated",
            Self::Deleted => "deleted",
        }
    }
}

const MAX_SCAN_CHANGES: usize = 500;

/// 扫描状态快照（供 `getScanStatus`）。
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ScanStatus {
    /// 当前是否正在扫描。
    pub scanning: bool,
    /// 本轮已处理（新增+更新+跳过）的对象数。
    pub scanned: u32,
    /// 上次成功扫描完成时间（`datetime('now')` 文本）。
    pub last_scan_at: Option<String>,
    /// 上次扫描的错误信息（若有）。
    pub error: Option<String>,
}

/// 扫描器：绑定对象存储与索引。
pub struct Scanner {
    store: Arc<dyn ObjectStore>,
    index: Index,
    concurrency: usize,
    status: Arc<Mutex<ScanStatus>>,
}

impl Scanner {
    /// 用默认并发上限构建。
    pub fn new(store: Arc<dyn ObjectStore>, index: Index) -> Self {
        Self::with_concurrency(store, index, DEFAULT_CONCURRENCY)
    }

    /// 指定并发头部解析上限构建。
    ///
    /// # Panics
    /// `concurrency` 为 0 时 panic。
    pub fn with_concurrency(store: Arc<dyn ObjectStore>, index: Index, concurrency: usize) -> Self {
        assert!(concurrency > 0, "concurrency 必须为正");
        Self {
            store,
            index,
            concurrency,
            status: Arc::new(Mutex::new(ScanStatus::default())),
        }
    }

    /// 读取当前扫描状态快照。
    pub fn scan_status(&self) -> ScanStatus {
        self.status.lock().unwrap().clone()
    }

    /// 增量扫描 `prefix`（`None` 表示整个 bucket）。返回本轮统计。
    pub async fn scan(&self, prefix: Option<&str>) -> Result<ScanReport> {
        let guard = self.begin_scan().ok_or(Error::AlreadyScanning)?;
        self.finish_started_scan(prefix.unwrap_or(""), guard).await
    }

    /// 按已知 `key` 直接读取对象头部解析入库，**绕过 bucket 列举**。
    ///
    /// 用于上传后的即时入库：Garage 写后 LIST 存在可见性延迟，依赖列举的
    /// [`scan`](Self::scan) 会漏掉刚写入对象；本方法按 key 直接 `head` + 读文件头，
    /// 对「读己之写」可靠（S3 新对象 `head`/`get` 强一致，仅 LIST 最终一致）。
    ///
    /// 语义为单个对象与索引的一次对账（reconcile），因此也可用于失败补偿后恢复索引：
    /// - 对象存在且为音频后缀 → 只读文件头解析、抽封面、按 `object_key` upsert；
    /// - 对象不存在 → 从索引删除该 key（若存在），返回 `deleted`；
    /// - 对象存在但非音频后缀 → 不入库，返回空统计。
    ///
    /// 不占用扫描 single-flight 状态：入库按 `object_key` 幂等，与并发全量扫描安全共存。
    pub async fn ingest_object(&self, key: &str) -> Result<ScanReport> {
        let meta = match self.store.head(key).await {
            Ok(meta) => meta,
            Err(StorageError::NotFound(_)) => {
                let deleted = self.index.media().delete_by_object_key(key).await?;
                return Ok(ScanReport {
                    deleted: u32::from(deleted),
                    ..Default::default()
                });
            }
            Err(error) => return Err(error.into()),
        };
        if !is_audio(key) {
            return Ok(ScanReport::default());
        }
        let entry = ListEntry {
            key: key.to_string(),
            etag: meta.etag,
            size: meta.size,
        };
        let was_present = incremental::existing_tracks(self.index.pool(), key)
            .await?
            .contains_key(key);
        // 只读文件头：读取上限内的前缀字节，绝不整读音频（红线）。
        let cap = entry.size.min(HEADER_READ_CAP);
        let head = self.store.get_range(&entry.key, 0..cap).await?;
        let parsed = tags::parse_header(head)
            .map_err(|error| Error::Parse(format!("{}: {error}", entry.key)))?;
        let cover_key = match &parsed.cover {
            Some(cover) => Some(cover::store_cover(self.store.as_ref(), cover).await?),
            None => None,
        };
        let track =
            incremental::upsert_track(&self.index, &entry, &parsed, cover_key.as_deref()).await?;
        Ok(ScanReport {
            added: u32::from(!was_present),
            updated: u32::from(was_present),
            changes: vec![ScanChange {
                action: if was_present {
                    ScanAction::Updated
                } else {
                    ScanAction::Added
                },
                object_key: key.to_owned(),
                track,
            }],
            ..Default::default()
        })
    }

    /// 原子检查并后台启动一次扫描；已有扫描时返回 `false`。
    pub fn try_start(self: &Arc<Self>, prefix: Option<String>) -> bool {
        let Some(guard) = self.begin_scan() else {
            return false;
        };
        let scanner = self.clone();
        tokio::spawn(async move {
            if let Err(error) = scanner
                .finish_started_scan(prefix.as_deref().unwrap_or(""), guard)
                .await
            {
                tracing::error!(%error, "后台扫描失败");
            }
        });
        true
    }

    fn begin_scan(&self) -> Option<ScanGuard> {
        let mut status = self.status.lock().unwrap();
        if status.scanning {
            return None;
        }
        status.scanning = true;
        status.scanned = 0;
        status.error = None;
        Some(ScanGuard {
            status: self.status.clone(),
        })
    }

    async fn finish_started_scan(&self, prefix: &str, guard: ScanGuard) -> Result<ScanReport> {
        let result = self.run_scan(prefix).await;
        {
            let mut status = self.status.lock().unwrap();
            if let Err(e) = &result {
                status.error = Some(e.to_string());
            }
        }
        drop(guard);
        result
    }

    /// 扫描主体：列举 → 分类 → 并发解析 → 顺序写库 → 删除 → 更新断点。
    async fn run_scan(&self, prefix: &str) -> Result<ScanReport> {
        let pool = self.index.pool();
        let existing = incremental::existing_tracks(pool, prefix).await?;

        // 列举 bucket，按 (key, etag) 分类为「待入库」与「未变」；记录已见 key。
        let mut seen: HashSet<String> = HashSet::new();
        let mut to_ingest: Vec<ListEntry> = Vec::new();
        let mut unchanged = 0u32;
        let mut token: Option<String> = None;
        loop {
            let page = self.store.list(prefix, token.clone()).await?;
            for entry in page.entries {
                if !is_audio(&entry.key) {
                    continue;
                }
                seen.insert(entry.key.clone());
                match existing.get(&entry.key) {
                    // etag 均存在且相等 → 未变，跳过。
                    Some((_, db_etag))
                        if entry.etag.is_some() && db_etag.as_deref() == entry.etag.as_deref() =>
                    {
                        unchanged += 1;
                    }
                    // 新增或 etag 变化 → 待入库。
                    _ => to_ingest.push(entry),
                }
            }
            // 持久化断点游标；到末页则清空游标退出。
            incremental::set_cursor(pool, page.next_token.as_deref()).await?;
            match page.next_token {
                Some(t) => token = Some(t),
                None => break,
            }
        }

        // 并发（信号量限流）读文件头 + 解析 + 抽封面，DB 写入随后顺序进行。
        let ingests = self.parse_all(to_ingest).await?;

        let mut added = 0u32;
        let mut updated = 0u32;
        let mut changes = Vec::new();
        let mut changes_truncated = false;
        for ingest in ingests {
            let was_present = existing.contains_key(&ingest.entry.key);
            let track = incremental::upsert_track(
                &self.index,
                &ingest.entry,
                &ingest.meta,
                ingest.cover_key.as_deref(),
            )
            .await?;
            push_change(
                &mut changes,
                &mut changes_truncated,
                ScanChange {
                    action: if was_present {
                        ScanAction::Updated
                    } else {
                        ScanAction::Added
                    },
                    object_key: ingest.entry.key.clone(),
                    track,
                },
            );
            if was_present {
                updated += 1;
            } else {
                added += 1;
            }
            self.status.lock().unwrap().scanned += 1;
        }

        // 删除：DB 有（前缀内）而 bucket 无。
        let mut deleted = 0u32;
        for key in existing.keys() {
            if !seen.contains(key) {
                let track = self.index.media().get_track(existing[key].0).await?;
                self.index.media().delete_by_object_key(key).await?;
                deleted += 1;
                if let Some(track) = track {
                    push_change(
                        &mut changes,
                        &mut changes_truncated,
                        ScanChange {
                            action: ScanAction::Deleted,
                            object_key: key.clone(),
                            track,
                        },
                    );
                }
            }
        }

        incremental::finish_scan(pool).await?;
        let last = incremental::last_scan_at(pool).await?;
        {
            let mut status = self.status.lock().unwrap();
            status.scanned += unchanged;
            status.last_scan_at = last;
        }

        Ok(ScanReport {
            added,
            updated,
            deleted,
            unchanged,
            changes,
            changes_truncated,
        })
    }

    /// 并发解析待入库项：每个任务经信号量限流，只读文件头 + 抽封面。
    async fn parse_all(&self, entries: Vec<ListEntry>) -> Result<Vec<Ingest>> {
        let semaphore = Arc::new(Semaphore::new(self.concurrency));
        let mut handles = Vec::with_capacity(entries.len());
        for entry in entries {
            let store = self.store.clone();
            let semaphore = semaphore.clone();
            handles.push(tokio::spawn(async move {
                let _permit = semaphore.acquire_owned().await.expect("信号量不应被关闭");
                // 只读文件头：读取上限内的前缀字节，绝不整读音频（红线）。
                let cap = entry.size.min(HEADER_READ_CAP);
                let head = store.get_range(&entry.key, 0..cap).await?;
                let meta = tags::parse_header(head)
                    .map_err(|e| Error::Parse(format!("{}: {e}", entry.key)))?;
                let cover_key = match &meta.cover {
                    Some(cover) => Some(cover::store_cover(store.as_ref(), cover).await?),
                    None => None,
                };
                Ok::<Ingest, Error>(Ingest {
                    entry,
                    meta,
                    cover_key,
                })
            }));
        }

        let mut ingests = Vec::with_capacity(handles.len());
        for handle in handles {
            ingests.push(handle.await.expect("解析任务不应 panic")?);
        }
        Ok(ingests)
    }
}

fn push_change(changes: &mut Vec<ScanChange>, truncated: &mut bool, change: ScanChange) {
    if changes.len() < MAX_SCAN_CHANGES {
        changes.push(change);
    } else {
        *truncated = true;
    }
}

/// 扫描 single-flight 的取消安全守卫；future 被 drop 时同样复位状态。
struct ScanGuard {
    status: Arc<Mutex<ScanStatus>>,
}

impl Drop for ScanGuard {
    fn drop(&mut self) {
        self.status.lock().unwrap().scanning = false;
    }
}

/// 一条待写入索引的解析结果。
struct Ingest {
    entry: ListEntry,
    meta: tags::ParsedTrack,
    cover_key: Option<String>,
}

/// 依扩展名判断是否为可入库音频。
fn is_audio(key: &str) -> bool {
    if key.starts_with("transcode/") || key.starts_with("covers/") {
        return false;
    }
    match key.rsplit_once('.') {
        Some((_, ext)) => {
            let ext = ext.to_ascii_lowercase();
            AUDIO_EXTS.contains(&ext.as_str())
        }
        None => false,
    }
}
