//! FFmpeg 按需转码管线。
//!
//! 原曲与缓存对象均经 Range 分块读取；未命中时实时把 FFmpeg stdout 发给调用方并写入
//! 自动清理的临时文件。只有调用方完整消费、输入完成且 FFmpeg 成功退出后，才流式上传
//! Garage 并写 SQLite 登记。调用方中断会终止子进程且不会留下缓存。

use std::fmt;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use bytes::Bytes;
use futures::Stream;
use tokio::sync::{watch, Mutex, Semaphore};

use crate::index::Index;
use crate::storage::{ObjectStore, StorageError};

mod cache;
mod decision;
mod ffmpeg;
mod stream;

use stream::{channel, object_stream, pump_object, send_error, send_terminal};

pub use cache::cache_key;
pub use decision::should_transcode;

/// 转码模块结果类型。
pub type Result<T> = std::result::Result<T, Error>;

/// 对外返回的有界异步字节流。
pub type ByteStream = Pin<Box<dyn Stream<Item = Result<Bytes>> + Send>>;

/// 转码错误。
#[derive(Debug)]
pub enum Error {
    /// 对象存储读写失败。
    Storage(StorageError),
    /// SQLite 缓存登记失败。
    Index(sqlx::Error),
    /// 子进程、管道或临时文件 I/O 失败。
    Io(std::io::Error),
    /// FFmpeg 非零退出。
    FfmpegFailed,
    /// 不支持的目标格式。
    UnsupportedFormat(String),
    /// 内部异步任务异常退出。
    Task(String),
    /// SQLite 登记失败且 Garage 对象补偿删除也失败。
    CacheCompensation {
        /// 需要人工对账的对象键。
        object_key: String,
        /// SQLite 主错误。
        index_error: String,
        /// Garage 补偿错误。
        cleanup_error: String,
    },
    /// 主动回滚缓存时 SQLite/Garage 至少一侧清理失败。
    CacheCleanup {
        /// 需要人工对账的对象键。
        object_key: String,
        /// SQLite 清理错误（若有）。
        index_error: Option<String>,
        /// Garage 清理错误（若有）。
        storage_error: Option<String>,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Storage(error) => write!(f, "{error}"),
            Error::Index(error) => write!(f, "缓存索引错误: {error}"),
            Error::Io(error) => write!(f, "转码 I/O 错误: {error}"),
            Error::FfmpegFailed => write!(f, "FFmpeg 转码失败"),
            Error::UnsupportedFormat(format) => write!(f, "不支持的转码格式: {format}"),
            Error::Task(error) => write!(f, "转码任务异常: {error}"),
            Error::CacheCompensation {
                object_key,
                index_error,
                cleanup_error,
            } => write!(
                f,
                "缓存登记失败且补偿删除失败: object_key={object_key}, SQLite={index_error}, Garage={cleanup_error}"
            ),
            Error::CacheCleanup {
                object_key,
                index_error,
                storage_error,
            } => write!(
                f,
                "缓存回滚未完全成功: object_key={object_key}, SQLite={}, Garage={}",
                index_error.as_deref().unwrap_or("ok"),
                storage_error.as_deref().unwrap_or("ok")
            ),
        }
    }
}

impl std::error::Error for Error {}

impl From<StorageError> for Error {
    fn from(value: StorageError) -> Self {
        Self::Storage(value)
    }
}

impl From<sqlx::Error> for Error {
    fn from(value: sqlx::Error) -> Self {
        Self::Index(value)
    }
}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

/// 转码所需的曲目存储信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscodeTrack {
    /// SQLite 曲目主键。
    pub id: i64,
    /// Garage 原始对象键。
    pub object_key: String,
    /// 原始编码/后缀。
    pub codec: String,
    /// 原始码率（kbps，未知为 0）。
    pub bitrate: u32,
}

impl TranscodeTrack {
    /// 构造曲目转码输入。
    pub fn new(
        id: i64,
        object_key: impl Into<String>,
        codec: impl Into<String>,
        bitrate: u32,
    ) -> Self {
        Self {
            id,
            object_key: object_key.into(),
            codec: codec.into().to_ascii_lowercase(),
            bitrate,
        }
    }
}

/// 客户端请求的目标格式与最大码率。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscodeTarget {
    /// 目标格式（`raw`、`aac` 或 `opus`）。
    pub format: String,
    /// 目标码率（kbps；`raw` 可为 0）。
    pub bitrate: u32,
}

impl TranscodeTarget {
    /// 构造并规范化目标格式。
    pub fn new(format: impl Into<String>, bitrate: u32) -> Self {
        Self {
            format: format.into().to_ascii_lowercase(),
            bitrate,
        }
    }
}

/// FFmpeg 转码服务；可克隆并在请求间共享信号量。
#[derive(Clone)]
pub struct Transcoder {
    store: Arc<dyn ObjectStore>,
    index: Index,
    ffmpeg_path: PathBuf,
    permits: Arc<Semaphore>,
    cache_locks: CacheKeyLocks,
}

#[derive(Clone)]
struct CacheKeyLocks {
    shards: Arc<Vec<Arc<Mutex<()>>>>,
}

impl CacheKeyLocks {
    fn new() -> Self {
        Self {
            shards: Arc::new((0..64).map(|_| Arc::new(Mutex::new(()))).collect()),
        }
    }

    fn for_key(&self, key: &str) -> Arc<Mutex<()>> {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        self.shards[hasher.finish() as usize % self.shards.len()].clone()
    }
}

impl Transcoder {
    /// 以默认最多 2 个 FFmpeg 子进程创建服务。
    pub fn new(store: Arc<dyn ObjectStore>, index: Index, ffmpeg_path: impl Into<PathBuf>) -> Self {
        Self::with_concurrency(store, index, ffmpeg_path, 2)
    }

    /// 以指定 FFmpeg 并发上限创建服务。
    ///
    /// # Panics
    /// `concurrency` 为 0 时 panic。
    pub fn with_concurrency(
        store: Arc<dyn ObjectStore>,
        index: Index,
        ffmpeg_path: impl Into<PathBuf>,
        concurrency: usize,
    ) -> Self {
        assert!(concurrency > 0, "FFmpeg 并发上限必须为正");
        Self {
            store,
            index,
            ffmpeg_path: ffmpeg_path.into(),
            permits: Arc::new(Semaphore::new(concurrency)),
            cache_locks: CacheKeyLocks::new(),
        }
    }

    /// 返回原曲、缓存或实时 FFmpeg 输出的有界字节流。
    ///
    /// 本方法只完成缓存判定与任务启动；所有大对象内容均在返回的流被轮询时分块传输。
    pub async fn stream(
        &self,
        track: TranscodeTrack,
        target: TranscodeTarget,
    ) -> Result<ByteStream> {
        if !should_transcode(&track, &target) {
            let size = self.store.head(&track.object_key).await?.size;
            return Ok(object_stream(self.store.clone(), track.object_key, size));
        }

        if target.format != "aac" && target.format != "opus" {
            return Err(Error::UnsupportedFormat(target.format));
        }
        Ok(self.transcode_stream(track, target))
    }

    fn transcode_stream(&self, track: TranscodeTrack, target: TranscodeTarget) -> ByteStream {
        let (tx, output) = channel();
        let store = self.store.clone();
        let index = self.index.clone();
        let ffmpeg_path = self.ffmpeg_path.clone();
        let permits = self.permits.clone();
        let cache_locks = self.cache_locks.clone();
        tokio::spawn(async move {
            let cache_key = cache::cache_key(track.id, &target);
            let key_lock = cache_locks.for_key(&cache_key);
            let _key_guard = tokio::select! {
                _ = tx.closed() => return,
                guard = key_lock.lock_owned() => guard,
            };
            match cached_object(&index, store.as_ref(), track.id, &target).await {
                Ok(Some((key, size))) => {
                    if let Err(error) = pump_object(store, key, size, &tx).await {
                        send_error(&tx, error).await;
                    }
                    return;
                }
                Ok(None) => {}
                Err(error) => {
                    send_error(&tx, error).await;
                    return;
                }
            }
            let permit = tokio::select! {
                _ = tx.closed() => return,
                result = permits.acquire_owned() => match result {
                    Ok(permit) => permit,
                    Err(error) => {
                        send_error(&tx, Error::Task(error.to_string())).await;
                        return;
                    }
                },
            };
            let result = ffmpeg::run(store.clone(), &ffmpeg_path, &track, &target, &tx).await;
            drop(permit);
            match result {
                Ok(Some(temp)) => {
                    let Ok(finalized) = send_terminal(&tx).await else {
                        return;
                    };
                    let persist_store = store.clone();
                    let persist_index = index.clone();
                    let persist_target = target.clone();
                    let (cancel_persist, cancelled) = watch::channel(false);
                    let persist = tokio::spawn(async move {
                        let _temp = temp;
                        cache::persist(
                            persist_store,
                            &persist_index,
                            _temp.path(),
                            track.id,
                            &persist_target,
                            cancelled,
                        )
                        .await
                    });
                    tokio::pin!(persist);
                    let persisted = tokio::select! {
                        biased;
                        _ = tx.closed() => {
                            let _ = cancel_persist.send(true);
                            match persist.await {
                                Ok(Ok(_)) => {}
                                Ok(Err(error)) => {
                                    tracing::error!(error = %error, track_id = track.id, "取消后的缓存任务失败");
                                }
                                Err(error) => {
                                    tracing::error!(error = %error, track_id = track.id, "取消后的缓存任务异常退出");
                                }
                            }
                            if let Err(error) = cache::discard(store.clone(), &index, track.id, &target).await {
                                tracing::error!(error = %error, track_id = track.id, "调用方中断后的缓存回滚失败");
                            }
                            return;
                        }
                        result = &mut persist => result,
                    };
                    match persisted {
                        Ok(Ok(cache::PersistOutcome::Committed)) => {}
                        Ok(Ok(cache::PersistOutcome::Cancelled)) => {
                            tracing::warn!(
                                track_id = track.id,
                                "缓存任务在调用方仍连接时收到取消状态"
                            );
                        }
                        Ok(Err(error)) => {
                            tracing::error!(error = %error, track_id = track.id, "转码缓存提交失败");
                            if let Err(cleanup) =
                                cache::discard(store.clone(), &index, track.id, &target).await
                            {
                                tracing::error!(error = %cleanup, track_id = track.id, "转码缓存提交失败后的再次回滚失败");
                            }
                        }
                        Err(error) => {
                            tracing::error!(error = %error, track_id = track.id, "转码缓存任务异常退出");
                            if let Err(cleanup) =
                                cache::discard(store.clone(), &index, track.id, &target).await
                            {
                                tracing::error!(error = %cleanup, track_id = track.id, "异常缓存任务的回滚失败");
                            }
                        }
                    }
                    if finalized.send(()).is_err() {
                        if let Err(error) =
                            cache::discard(store.clone(), &index, track.id, &target).await
                        {
                            tracing::error!(error = %error, track_id = track.id, "终点确认失败后的缓存回滚失败");
                        }
                    }
                }
                Ok(None) => {}
                Err(error) => send_error(&tx, error).await,
            }
        });
        output
    }
}

async fn cached_object(
    index: &Index,
    store: &dyn ObjectStore,
    track_id: i64,
    target: &TranscodeTarget,
) -> Result<Option<(String, u64)>> {
    let Some(entry) = index
        .transcode_cache()
        .get(track_id, &target.format, target.bitrate)
        .await?
    else {
        return Ok(None);
    };
    match store.head(&entry.object_key).await {
        Ok(meta) => Ok(Some((entry.object_key, meta.size))),
        Err(StorageError::NotFound(_)) => {
            index
                .transcode_cache()
                .remove(track_id, &target.format, target.bitrate)
                .await?;
            Ok(None)
        }
        Err(error) => Err(error.into()),
    }
}
