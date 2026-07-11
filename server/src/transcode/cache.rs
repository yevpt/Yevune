//! 转码缓存键与成功产物登记。

use std::path::Path;
use std::sync::Arc;

use crate::index::{Index, NewTranscodeCache};
use crate::storage::ObjectStore;
use tokio::sync::watch;

use super::{Error, TranscodeTarget};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PersistOutcome {
    Committed,
    Cancelled,
}

/// 按约定生成 Garage 转码缓存键。
pub fn cache_key(track_id: i64, target: &TranscodeTarget) -> String {
    format!(
        "transcode/{track_id}/{}_{}.{}",
        target.format, target.bitrate, target.format
    )
}

pub(crate) async fn persist(
    store: Arc<dyn ObjectStore>,
    index: &Index,
    path: &Path,
    track_id: i64,
    target: &TranscodeTarget,
    mut cancelled: watch::Receiver<bool>,
) -> Result<PersistOutcome, Error> {
    let key = cache_key(track_id, target);
    let meta = tokio::select! {
        biased;
        _ = wait_for_cancellation(&mut cancelled) => return Ok(PersistOutcome::Cancelled),
        result = store.put_file(&key, path) => result?,
    };
    let entry = NewTranscodeCache {
        track_id,
        format: target.format.clone(),
        bitrate: target.bitrate,
        object_key: key.clone(),
        size: meta.size,
    };
    if let Err(error) = index.transcode_cache().upsert(&entry).await {
        if let Err(cleanup) = store.delete(&key).await {
            tracing::error!(
                object_key = %key,
                index_error = %error,
                cleanup_error = %cleanup,
                "SQLite 缓存登记失败且 Garage 补偿删除失败"
            );
            return Err(Error::CacheCompensation {
                object_key: key,
                index_error: error.to_string(),
                cleanup_error: cleanup.to_string(),
            });
        }
        return Err(Error::Index(error));
    }
    if *cancelled.borrow() {
        discard(store, index, track_id, target).await?;
        return Ok(PersistOutcome::Cancelled);
    }
    Ok(PersistOutcome::Committed)
}

async fn wait_for_cancellation(cancelled: &mut watch::Receiver<bool>) {
    loop {
        if *cancelled.borrow() {
            return;
        }
        if cancelled.changed().await.is_err() {
            std::future::pending::<()>().await;
        }
    }
}

pub(crate) async fn discard(
    store: Arc<dyn ObjectStore>,
    index: &Index,
    track_id: i64,
    target: &TranscodeTarget,
) -> Result<(), Error> {
    let key = cache_key(track_id, target);
    let index_error = index
        .transcode_cache()
        .remove(track_id, &target.format, target.bitrate)
        .await
        .err()
        .map(|error| error.to_string());
    let storage_error = store
        .delete(&key)
        .await
        .err()
        .map(|error| error.to_string());
    if index_error.is_none() && storage_error.is_none() {
        return Ok(());
    }
    tracing::error!(
        object_key = %key,
        index_error = ?index_error,
        storage_error = ?storage_error,
        "缓存回滚未完全成功"
    );
    Err(Error::CacheCleanup {
        object_key: key,
        index_error,
        storage_error,
    })
}

#[cfg(test)]
mod tests {
    use std::ops::Range;
    use std::path::Path;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    use async_trait::async_trait;
    use bytes::Bytes;

    use crate::index::NewTrack;
    use crate::storage::{
        ListPage, MemoryStore, ObjectMeta, ObjectStore, Result as StoreResult, StorageError,
    };

    use super::{discard, persist, PersistOutcome};
    use crate::transcode::{Error, TranscodeTarget};

    struct CleanupFailStore {
        delete_called: AtomicBool,
    }

    #[async_trait]
    impl ObjectStore for CleanupFailStore {
        async fn list(&self, _prefix: &str, _token: Option<String>) -> StoreResult<ListPage> {
            unreachable!()
        }
        async fn get(&self, _key: &str) -> StoreResult<Bytes> {
            unreachable!()
        }
        async fn get_range(&self, _key: &str, _range: Range<u64>) -> StoreResult<Bytes> {
            unreachable!()
        }
        async fn put(&self, _key: &str, _bytes: Bytes) -> StoreResult<ObjectMeta> {
            unreachable!()
        }
        async fn put_file(&self, _key: &str, _path: &Path) -> StoreResult<ObjectMeta> {
            Ok(ObjectMeta {
                etag: Some("etag".into()),
                size: 7,
            })
        }
        async fn delete(&self, key: &str) -> StoreResult<()> {
            self.delete_called.store(true, Ordering::SeqCst);
            Err(StorageError::Backend(format!("delete failed: {key}")))
        }
        async fn head(&self, _key: &str) -> StoreResult<ObjectMeta> {
            unreachable!()
        }
    }

    async fn temp_index() -> (crate::index::Index, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let index = crate::index::Index::connect(&dir.path().join("index.sqlite"))
            .await
            .unwrap();
        (index, dir)
    }

    #[tokio::test]
    async fn sqlite_register_失败且对象删除失败会返回可对账错误() {
        let (index, _dir) = temp_index().await;
        let track_id = index
            .media()
            .upsert_track(&NewTrack {
                title: "gone".into(),
                object_key: "music/gone.flac".into(),
                ..Default::default()
            })
            .await
            .unwrap();
        sqlx::query("DELETE FROM tracks WHERE id = ?")
            .bind(track_id)
            .execute(index.pool())
            .await
            .unwrap();
        let store = Arc::new(CleanupFailStore {
            delete_called: AtomicBool::new(false),
        });
        let temp = tempfile::NamedTempFile::new().unwrap();

        let (_cancel, cancelled) = tokio::sync::watch::channel(false);
        let error = persist(
            store.clone(),
            &index,
            temp.path(),
            track_id,
            &TranscodeTarget::new("opus", 96),
            cancelled,
        )
        .await
        .unwrap_err();

        assert!(matches!(error, Error::CacheCompensation { .. }));
        assert!(store.delete_called.load(Ordering::SeqCst));
        assert!(error.to_string().contains("transcode/"));
    }

    #[tokio::test]
    async fn discard_同时报告_sqlite_与对象删除失败() {
        let (index, _dir) = temp_index().await;
        index.pool().close().await;
        let store = Arc::new(CleanupFailStore {
            delete_called: AtomicBool::new(false),
        });

        let error = discard(store.clone(), &index, 42, &TranscodeTarget::new("opus", 96))
            .await
            .unwrap_err();

        assert!(matches!(error, Error::CacheCleanup { .. }));
        assert!(store.delete_called.load(Ordering::SeqCst));
        let message = error.to_string();
        assert!(message.contains("SQLite"));
        assert!(message.contains("Garage"));
    }

    #[tokio::test]
    async fn object_complete_后取消会等待_locked_upsert_结束再补偿() {
        use std::time::Duration;

        use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
        use tokio::sync::watch;

        let (migrated, dir) = temp_index().await;
        let db_path = dir.path().join("index.sqlite");
        migrated.pool().close().await;
        let options = SqliteConnectOptions::new()
            .filename(&db_path)
            .create_if_missing(false)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(2)
            .connect_with(options)
            .await
            .unwrap();
        let index = crate::index::Index::from_pool_for_test(pool);
        let track_id = index
            .media()
            .upsert_track(&NewTrack {
                title: "locked".into(),
                object_key: "music/locked.flac".into(),
                ..Default::default()
            })
            .await
            .unwrap();
        let mut lock = index.pool().acquire().await.unwrap();
        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut *lock)
            .await
            .unwrap();
        let store = Arc::new(MemoryStore::new());
        let output = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(output.path(), b"encoded").unwrap();
        let target = TranscodeTarget::new("opus", 96);
        let key = super::cache_key(track_id, &target);
        let (cancel, cancelled) = watch::channel(false);
        let persist_store = store.clone();
        let persist_index = index.clone();
        let persist_target = target.clone();
        let mut task = tokio::spawn(async move {
            let _output = output;
            persist(
                persist_store,
                &persist_index,
                _output.path(),
                track_id,
                &persist_target,
                cancelled,
            )
            .await
        });
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if store.head(&key).await.is_ok() {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("对象应先完成上传");

        cancel.send(true).unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(100), &mut task)
                .await
                .is_err(),
            "对象 complete 后不得取消正在等待锁的 SQLite 登记"
        );
        sqlx::query("ROLLBACK").execute(&mut *lock).await.unwrap();
        drop(lock);

        let outcome = task.await.unwrap().unwrap();
        assert_eq!(outcome, PersistOutcome::Cancelled);
        assert!(index
            .transcode_cache()
            .get(track_id, "opus", 96)
            .await
            .unwrap()
            .is_none());
        assert!(matches!(
            store.head(&key).await,
            Err(StorageError::NotFound(_))
        ));
    }
}
