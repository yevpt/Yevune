//! 曲库对象变更的 owned 终态任务与补偿逻辑。

use std::path::Path;

use lofty::config::WriteOptions;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::prelude::{Accessor, ItemKey};
use lofty::probe::Probe;
use tempfile::NamedTempFile;
use tokio::io::AsyncWriteExt;

use crate::api::AppState;
use crate::storage::{StorageError, STREAM_CHUNK_SIZE};

use super::TagParams;

#[derive(Debug)]
pub(super) enum OperationError {
    NotFound,
    DestinationExists,
    InvalidTags,
    Internal,
}

fn operation_keys(id: i64, object_keys: &[&str]) -> Vec<String> {
    let mut keys = Vec::with_capacity(object_keys.len() + 1);
    keys.push(format!("track:{id}"));
    keys.extend(object_keys.iter().map(|key| format!("object:{key}")));
    keys
}

pub(super) async fn commit_upload(
    state: AppState,
    key: String,
    temp: NamedTempFile,
) -> Result<i64, OperationError> {
    let _guards = state
        .library_operation_locks
        .lock([format!("object:{key}")])
        .await;
    let backup = match state.store.head(&key).await {
        Ok(_) => match download_to_temp(&state, &key).await {
            Ok(value) => Some(value),
            Err(error) => {
                tracing::error!(%error, object_key = %key, "覆盖上传前备份旧对象失败");
                return Err(OperationError::Internal);
            }
        },
        Err(StorageError::NotFound(_)) => None,
        Err(error) => {
            tracing::error!(%error, object_key = %key, "检查上传目标失败");
            return Err(OperationError::Internal);
        }
    };
    if let Err(error) = state.store.put_file(&key, temp.path()).await {
        tracing::error!(%error, object_key = %key, "上传对象失败");
        return Err(OperationError::Internal);
    }
    if let Err(error) = scan_key(&state, &key).await {
        tracing::error!(%error, object_key = %key, "上传后即时入库失败");
        let compensated = match &backup {
            Some(backup) => state.store.put_file(&key, backup.path()).await.map(|_| ()),
            None => state.store.delete(&key).await,
        };
        if let Err(cleanup) = compensated {
            tracing::error!(%cleanup, object_key = %key, "上传入库失败且对象补偿失败，需要人工对账");
        } else if let Err(rescan) = scan_key(&state, &key).await {
            tracing::error!(%rescan, object_key = %key, "上传对象已补偿但索引恢复失败，需要人工对账");
        }
        return Err(OperationError::Internal);
    }
    match sqlx::query_scalar("SELECT id FROM tracks WHERE object_key = ?")
        .bind(&key)
        .fetch_optional(state.index.pool())
        .await
    {
        Ok(Some(id)) => Ok(id),
        Ok(None) => {
            tracing::error!(object_key = %key, "上传扫描成功但索引中没有曲目");
            Err(OperationError::Internal)
        }
        Err(error) => {
            tracing::error!(%error, object_key = %key, "读取上传曲目失败");
            Err(OperationError::Internal)
        }
    }
}

pub(super) async fn commit_write_back(
    state: AppState,
    id: i64,
    params: TagParams,
) -> Result<(), OperationError> {
    let source = state
        .index
        .media()
        .media_source(id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "读取写回曲目源失败");
            OperationError::Internal
        })?
        .ok_or(OperationError::NotFound)?;
    let _guards = state
        .library_operation_locks
        .lock(operation_keys(id, &[&source.object_key]))
        .await;
    let current = state
        .index
        .media()
        .media_source(id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "锁内复查写回曲目源失败");
            OperationError::Internal
        })?
        .ok_or(OperationError::NotFound)?;
    if current.object_key != source.object_key || current.etag != source.etag {
        tracing::warn!(track_id = id, "写回等待锁期间曲目源已变化，请重试");
        return Err(OperationError::Internal);
    }
    let backup = download_to_temp(&state, &source.object_key)
        .await
        .map_err(|error| {
            tracing::error!(%error, "分块下载写回源文件失败");
            OperationError::Internal
        })?;
    let working = copy_temp(&backup).await.map_err(|error| {
        tracing::error!(%error, "复制写回工作文件失败");
        OperationError::Internal
    })?;
    let path = working.path().to_path_buf();
    let fields = tag_field_names(&params);
    match tokio::task::spawn_blocking(move || write_tags(&path, &params)).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            tracing::warn!(%error, "修改本地标签失败");
            return Err(OperationError::InvalidTags);
        }
        Err(error) => {
            tracing::error!(%error, "标签写回 blocking 任务失败");
            return Err(OperationError::Internal);
        }
    }
    if let Err(error) = state
        .store
        .put_file(&source.object_key, working.path())
        .await
    {
        tracing::error!(%error, "覆盖写回对象失败，旧对象保持不变");
        return Err(OperationError::Internal);
    }
    let commit = match scan_key(&state, &source.object_key).await {
        Ok(_) => state.index.media().clear_tag_overrides(id, &fields).await,
        Err(error) => {
            tracing::error!(%error, "写回成功但重新入库失败");
            return restore_write_back(&state, &source.object_key, &backup).await;
        }
    };
    if let Err(error) = commit {
        tracing::error!(%error, "清除已写回标签覆盖失败，开始恢复写回前状态");
        return restore_write_back(&state, &source.object_key, &backup).await;
    }
    Ok(())
}

async fn restore_write_back(
    state: &AppState,
    key: &str,
    backup: &NamedTempFile,
) -> Result<(), OperationError> {
    if let Err(restore) = state.store.put_file(key, backup.path()).await {
        tracing::error!(%restore, object_key = %key, "标签写回提交失败且旧对象恢复失败，需要人工对账");
    } else if let Err(rescan) = scan_key(state, key).await {
        tracing::error!(%rescan, object_key = %key, "旧对象已恢复但索引恢复扫描失败，需要人工对账");
    }
    Err(OperationError::Internal)
}

pub(super) async fn commit_delete(state: AppState, id: i64) -> Result<(), OperationError> {
    let source = state
        .index
        .media()
        .media_source(id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "读取待删曲目失败");
            OperationError::Internal
        })?
        .ok_or(OperationError::NotFound)?;
    let _guards = state
        .library_operation_locks
        .lock(operation_keys(id, &[&source.object_key]))
        .await;
    let current = state
        .index
        .media()
        .media_source(id)
        .await
        .map_err(|error| {
            tracing::error!(%error, "锁内复查待删曲目失败");
            OperationError::Internal
        })?
        .ok_or(OperationError::NotFound)?;
    if current.object_key != source.object_key || current.etag != source.etag {
        tracing::warn!(track_id = id, "删除等待锁期间曲目源已变化，请重试");
        return Err(OperationError::Internal);
    }
    let backup = download_to_temp(&state, &source.object_key)
        .await
        .map_err(|error| {
            tracing::error!(%error, "删除前备份源对象失败");
            OperationError::Internal
        })?;
    if let Err(error) = state.store.delete(&source.object_key).await {
        tracing::error!(%error, "删除源对象返回错误，检查对象是否实际删除");
        match state.store.head(&source.object_key).await {
            Err(StorageError::NotFound(_)) => {
                tracing::warn!(object_key = %source.object_key, "删除虽返回错误但源对象已不存在，继续提交索引删除");
            }
            Ok(_) => return Err(OperationError::Internal),
            Err(head_error) => {
                tracing::error!(%head_error, object_key = %source.object_key, "无法确认删除结果，恢复备份以保持索引可读");
                restore_deleted_object(&state, &source.object_key, &backup).await;
                return Err(OperationError::Internal);
            }
        }
    }
    let deleted = sqlx::query("DELETE FROM tracks WHERE id = ? AND object_key = ? AND etag IS ?")
        .bind(id)
        .bind(&source.object_key)
        .bind(source.etag.as_deref())
        .execute(state.index.pool())
        .await;
    match deleted {
        Ok(result) if result.rows_affected() == 1 => Ok(()),
        Ok(_) => {
            tracing::error!(track_id = id, "对象已删除但索引 CAS 未命中，开始补偿");
            restore_deleted_object(&state, &source.object_key, &backup).await;
            Err(OperationError::Internal)
        }
        Err(error) => {
            tracing::error!(%error, "对象已删除但索引删除失败，开始补偿");
            restore_deleted_object(&state, &source.object_key, &backup).await;
            Err(OperationError::Internal)
        }
    }
}

async fn restore_deleted_object(state: &AppState, key: &str, backup: &NamedTempFile) {
    if let Err(restore) = state.store.put_file(key, backup.path()).await {
        tracing::error!(%restore, object_key = %key, "删除失败补偿恢复对象失败，需要人工对账");
    }
}

pub(super) async fn commit_move(
    state: AppState,
    id: i64,
    new_key: String,
) -> Result<(), OperationError> {
    loop {
        let source = state
            .index
            .media()
            .media_source(id)
            .await
            .map_err(|error| {
                tracing::error!(%error, "读取待移动曲目失败");
                OperationError::Internal
            })?
            .ok_or(OperationError::NotFound)?;
        let guards = state
            .library_operation_locks
            .lock(operation_keys(id, &[&source.object_key, &new_key]))
            .await;
        let current = state
            .index
            .media()
            .media_source(id)
            .await
            .map_err(|error| {
                tracing::error!(%error, "锁内复查待移动曲目失败");
                OperationError::Internal
            })?
            .ok_or(OperationError::NotFound)?;
        if current.object_key != source.object_key || current.etag != source.etag {
            drop(guards);
            continue;
        }
        if source.object_key == new_key {
            return Ok(());
        }
        match state.store.head(&new_key).await {
            Ok(_) => return Err(OperationError::DestinationExists),
            Err(StorageError::NotFound(_)) => {}
            Err(error) => {
                tracing::error!(%error, "检查移动目标失败");
                return Err(OperationError::Internal);
            }
        }
        let temp = download_to_temp(&state, &source.object_key)
            .await
            .map_err(|error| {
                tracing::error!(%error, "分块下载待移动对象失败");
                OperationError::Internal
            })?;
        let meta = state
            .store
            .put_file(&new_key, temp.path())
            .await
            .map_err(|error| {
                tracing::error!(%error, "写移动目标对象失败");
                OperationError::Internal
            })?;
        let moved = state
            .index
            .media()
            .move_source_cas(
                id,
                &source.object_key,
                source.etag.as_deref(),
                &new_key,
                meta.etag.as_deref(),
                meta.size,
            )
            .await;
        match moved {
            Ok(true) => {}
            Ok(false) => {
                tracing::error!(track_id = id, "移动索引 CAS 未命中");
                cleanup_owned_destination(&state, &new_key, meta.etag.as_deref()).await;
                return Err(OperationError::Internal);
            }
            Err(error) => {
                tracing::error!(%error, "更新移动后对象键失败");
                cleanup_owned_destination(&state, &new_key, meta.etag.as_deref()).await;
                return Err(OperationError::Internal);
            }
        }
        if let Err(error) = state.store.delete(&source.object_key).await {
            tracing::error!(%error, "删除移动源对象返回错误，检查对象是否实际删除");
            match state.store.head(&source.object_key).await {
                Err(StorageError::NotFound(_)) => {
                    tracing::warn!(object_key = %source.object_key, "移动源删除虽返回错误但对象已不存在，保留已提交目标");
                    return Ok(());
                }
                Ok(source_meta) if source.etag.is_some() && source_meta.etag == source.etag => {}
                Ok(_) => {
                    tracing::error!(object_key = %source.object_key, "移动源仍存在但 ETag 无法确认属于本次源，保留已索引目标以避免误删，需要人工对账");
                    return Err(OperationError::Internal);
                }
                Err(head_error) => {
                    tracing::error!(%head_error, object_key = %source.object_key, "无法确认移动源删除结果，保留已索引目标以避免数据丢失，需要人工对账");
                    return Err(OperationError::Internal);
                }
            }
            tracing::error!("移动源确认仍为原版本，开始回滚");
            match state
                .index
                .media()
                .move_source_cas(
                    id,
                    &new_key,
                    meta.etag.as_deref(),
                    &source.object_key,
                    source.etag.as_deref(),
                    source.size.unwrap_or_default() as u64,
                )
                .await
            {
                Ok(true) => {
                    cleanup_owned_destination(&state, &new_key, meta.etag.as_deref()).await;
                }
                Ok(false) => tracing::error!(
                    track_id = id,
                    "移动索引回滚 CAS 未命中，保留当前索引对象以避免数据丢失，需要人工对账"
                ),
                Err(rollback) => {
                    tracing::error!(%rollback, "移动索引回滚失败，保留当前索引对象以避免数据丢失，需要人工对账")
                }
            }
            return Err(OperationError::Internal);
        }
        return Ok(());
    }
}

async fn cleanup_owned_destination(state: &AppState, key: &str, expected_etag: Option<&str>) {
    let Some(expected_etag) = expected_etag else {
        tracing::error!(object_key = %key, "移动补偿缺少本次 put ETag，无法证明对象所有权，需要人工对账");
        return;
    };
    match state.store.head(key).await {
        Ok(meta) if meta.etag.as_deref() == Some(expected_etag) => {
            if let Err(error) = state.store.delete(key).await {
                tracing::error!(%error, object_key = %key, "移动补偿清理 owned 目标对象失败，需要人工对账");
            }
        }
        Ok(_) => {
            tracing::error!(object_key = %key, "移动补偿目标 ETag 已变化，拒绝删除，需要人工对账")
        }
        Err(StorageError::NotFound(_)) => {}
        Err(error) => {
            tracing::error!(%error, object_key = %key, "移动补偿检查目标 ETag 失败，需要人工对账")
        }
    }
}

async fn scan_key(
    state: &AppState,
    key: &str,
) -> crate::scanner::Result<crate::scanner::ScanReport> {
    loop {
        match state.scanner.scan(Some(key)).await {
            Err(crate::scanner::Error::AlreadyScanning) => tokio::task::yield_now().await,
            result => return result,
        }
    }
}

fn tag_field_names(params: &TagParams) -> Vec<&'static str> {
    let mut fields = Vec::new();
    for (field, present) in [
        ("title", params.title.is_some()),
        ("album", params.album.is_some()),
        ("artist", params.artist.is_some()),
        ("genre", params.genre.is_some()),
        ("year", params.year.is_some()),
        ("track", params.track.is_some()),
        ("discNumber", params.disc_number.is_some()),
    ] {
        if present {
            fields.push(field);
        }
    }
    fields
}

fn write_tags(path: &Path, params: &TagParams) -> Result<(), String> {
    let mut tagged = Probe::open(path)
        .map_err(|error| error.to_string())?
        .guess_file_type()
        .map_err(|error| error.to_string())?
        .read()
        .map_err(|error| error.to_string())?;
    let tag = if tagged.primary_tag().is_some() {
        tagged.primary_tag_mut()
    } else {
        tagged.first_tag_mut()
    }
    .ok_or_else(|| "文件没有可写标签".to_string())?;
    if let Some(value) = &params.title {
        tag.set_title(value.clone());
    }
    if let Some(value) = &params.album {
        tag.set_album(value.clone());
    }
    if let Some(value) = &params.artist {
        tag.set_artist(value.clone());
    }
    if let Some(value) = &params.genre {
        tag.set_genre(value.clone());
    }
    if let Some(value) = params
        .year
        .as_deref()
        .and_then(|value| value.parse::<u32>().ok())
    {
        tag.insert_text(ItemKey::Year, value.to_string());
    }
    if let Some(value) = params.track.as_deref().and_then(|value| value.parse().ok()) {
        tag.set_track(value);
    }
    if let Some(value) = params
        .disc_number
        .as_deref()
        .and_then(|value| value.parse().ok())
    {
        tag.set_disk(value);
    }
    tagged
        .save_to_path(path, WriteOptions::default())
        .map_err(|error| error.to_string())
}

async fn download_to_temp(state: &AppState, key: &str) -> Result<NamedTempFile, String> {
    let meta = state
        .store
        .head(key)
        .await
        .map_err(|error| error.to_string())?;
    let temp = NamedTempFile::new().map_err(|error| error.to_string())?;
    let mut output = tokio::fs::File::create(temp.path())
        .await
        .map_err(|error| error.to_string())?;
    let mut start = 0_u64;
    while start < meta.size {
        let end = (start + STREAM_CHUNK_SIZE as u64).min(meta.size);
        let bytes = state
            .store
            .get_range(key, start..end)
            .await
            .map_err(|error| error.to_string())?;
        output
            .write_all(&bytes)
            .await
            .map_err(|error| error.to_string())?;
        start = end;
    }
    output.flush().await.map_err(|error| error.to_string())?;
    Ok(temp)
}

async fn copy_temp(source: &NamedTempFile) -> Result<NamedTempFile, String> {
    let target = NamedTempFile::new().map_err(|error| error.to_string())?;
    tokio::fs::copy(source.path(), target.path())
        .await
        .map_err(|error| error.to_string())?;
    Ok(target)
}
