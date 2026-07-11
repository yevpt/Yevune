//! Garage（S3 兼容）实现，基于 `object_store` 的 `AmazonS3`（见 [ADR-0005]）。
//!
//! 委托底层库完成实际 S3 交互，只做类型/错误映射与分页归一，不泄漏其类型给上层。
//!
//! [ADR-0005]: ../../../docs/adr/0005-object-store-over-aws-sdk.md

use std::ops::Range;
use std::path::Path as FsPath;

use async_trait::async_trait;
use bytes::Bytes;
use futures::StreamExt;
use object_store::aws::{AmazonS3, AmazonS3Builder};
use object_store::path::Path;
use object_store::{MultipartUpload, ObjectStore as _, ObjectStoreExt as _, PutPayload, PutResult};
use tokio::io::AsyncReadExt;

use super::{
    ListEntry, ListPage, ObjectMeta, ObjectStore, Result, StorageError, DEFAULT_PAGE_SIZE,
    STREAM_CHUNK_SIZE,
};

/// Garage/S3 连接配置。
#[derive(Debug, Clone)]
pub struct GarageConfig {
    /// S3 端点 URL（如 `http://garage:3900`）。
    pub endpoint: String,
    /// bucket 名。
    pub bucket: String,
    /// Access Key ID。
    pub access_key: String,
    /// Secret Access Key。
    pub secret_key: String,
    /// 区域标识（Garage/MinIO 任意，默认 `garage`）。
    pub region: String,
    /// 是否允许明文 HTTP（局域网小白友好，默认 `true`；见 spec §10）。
    pub allow_http: bool,
    /// 分页页大小。
    pub page_size: usize,
}

impl GarageConfig {
    /// 用必要凭证构造配置，其余取合理默认（region=`garage`、允许 HTTP、默认页大小）。
    pub fn new(
        endpoint: impl Into<String>,
        bucket: impl Into<String>,
        access_key: impl Into<String>,
        secret_key: impl Into<String>,
    ) -> Self {
        Self {
            endpoint: endpoint.into(),
            bucket: bucket.into(),
            access_key: access_key.into(),
            secret_key: secret_key.into(),
            region: "garage".to_string(),
            allow_http: true,
            page_size: DEFAULT_PAGE_SIZE,
        }
    }
}

/// Garage 对象存储客户端。
pub struct GarageStore {
    inner: AmazonS3,
    page_size: usize,
}

impl GarageStore {
    /// 依配置建立客户端。Garage/MinIO 采用 path-style 寻址。
    pub fn new(config: GarageConfig) -> Result<Self> {
        let inner = AmazonS3Builder::new()
            .with_endpoint(&config.endpoint)
            .with_bucket_name(&config.bucket)
            .with_access_key_id(&config.access_key)
            .with_secret_access_key(&config.secret_key)
            .with_region(&config.region)
            .with_allow_http(config.allow_http)
            // Garage/MinIO 用 path-style（非 virtual-hosted）寻址。
            .with_virtual_hosted_style_request(false)
            .build()
            .map_err(to_storage_err)?;
        Ok(Self {
            inner,
            page_size: config.page_size,
        })
    }
}

/// 映射底层错误：区分「不存在」与其它后端错误。
fn to_storage_err(e: object_store::Error) -> StorageError {
    match e {
        object_store::Error::NotFound { path, .. } => StorageError::NotFound(path),
        other => StorageError::Backend(other.to_string()),
    }
}

const MULTIPART_CHUNK_SIZE: usize = 5 * 1024 * 1024;

struct MultipartAbortGuard {
    upload: Option<Box<dyn MultipartUpload>>,
}

impl MultipartAbortGuard {
    fn new(upload: Box<dyn MultipartUpload>) -> Self {
        Self {
            upload: Some(upload),
        }
    }

    fn upload(&mut self) -> &mut Box<dyn MultipartUpload> {
        self.upload.as_mut().expect("multipart guard 已结束")
    }

    async fn abort(mut self) {
        if let Some(mut upload) = self.upload.take() {
            let _ = upload.abort().await;
        }
    }

    fn disarm(mut self) {
        self.upload.take();
    }
}

impl Drop for MultipartAbortGuard {
    fn drop(&mut self) {
        let Some(mut upload) = self.upload.take() else {
            return;
        };
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                let _ = upload.abort().await;
            });
        }
    }
}

async fn upload_multipart_file(
    upload: Box<dyn MultipartUpload>,
    file: &mut tokio::fs::File,
) -> object_store::Result<(PutResult, u64)> {
    let mut guard = MultipartAbortGuard::new(upload);
    let mut part = Vec::with_capacity(MULTIPART_CHUNK_SIZE);
    let mut read_buffer = vec![0_u8; STREAM_CHUNK_SIZE];
    let mut size = 0_u64;
    loop {
        let remaining = MULTIPART_CHUNK_SIZE - part.len();
        let read = match file
            .read(&mut read_buffer[..remaining.min(STREAM_CHUNK_SIZE)])
            .await
        {
            Ok(read) => read,
            Err(source) => {
                guard.abort().await;
                return Err(object_store::Error::Generic {
                    store: "local file",
                    source: Box::new(source),
                });
            }
        };
        if read == 0 {
            break;
        }
        part.extend_from_slice(&read_buffer[..read]);
        size += read as u64;
        if part.len() == MULTIPART_CHUNK_SIZE {
            let payload = PutPayload::from(Bytes::from(std::mem::take(&mut part)));
            if let Err(error) = guard.upload().put_part(payload).await {
                guard.abort().await;
                return Err(error);
            }
            part = Vec::with_capacity(MULTIPART_CHUNK_SIZE);
        }
    }
    if !part.is_empty() {
        if let Err(error) = guard
            .upload()
            .put_part(PutPayload::from(Bytes::from(part)))
            .await
        {
            guard.abort().await;
            return Err(error);
        }
    }
    match guard.upload().complete().await {
        Ok(result) => {
            guard.disarm();
            Ok((result, size))
        }
        Err(error) => {
            guard.abort().await;
            Err(error)
        }
    }
}

#[async_trait]
impl ObjectStore for GarageStore {
    async fn list(&self, prefix: &str, token: Option<String>) -> Result<ListPage> {
        let prefix_path = Path::from(prefix);
        let mut stream = match &token {
            Some(t) => self
                .inner
                .list_with_offset(Some(&prefix_path), &Path::from(t.as_str())),
            None => self.inner.list(Some(&prefix_path)),
        };

        let mut entries = Vec::new();
        let mut next_token = None;
        while let Some(item) = stream.next().await {
            let meta = item.map_err(to_storage_err)?;
            if entries.len() == self.page_size {
                // 尚有后续对象 → 以本页末尾 key 作为续页游标（start-after）。
                next_token = entries.last().map(|e: &ListEntry| e.key.clone());
                break;
            }
            entries.push(ListEntry {
                key: meta.location.to_string(),
                etag: meta.e_tag,
                size: meta.size,
            });
        }

        Ok(ListPage {
            entries,
            next_token,
        })
    }

    async fn get(&self, key: &str) -> Result<Bytes> {
        let result = self
            .inner
            .get(&Path::from(key))
            .await
            .map_err(to_storage_err)?;
        result.bytes().await.map_err(to_storage_err)
    }

    async fn get_range(&self, key: &str, range: Range<u64>) -> Result<Bytes> {
        self.inner
            .get_range(&Path::from(key), range)
            .await
            .map_err(to_storage_err)
    }

    async fn put(&self, key: &str, bytes: Bytes) -> Result<ObjectMeta> {
        let size = bytes.len() as u64;
        let result = self
            .inner
            .put(&Path::from(key), PutPayload::from(bytes))
            .await
            .map_err(to_storage_err)?;
        Ok(ObjectMeta {
            etag: result.e_tag,
            size,
        })
    }

    async fn put_file(&self, key: &str, path: &FsPath) -> Result<ObjectMeta> {
        let mut file = tokio::fs::File::open(path)
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;
        let file_size = file
            .metadata()
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?
            .len();
        if file_size == 0 {
            return self.put(key, Bytes::new()).await;
        }

        let upload = self
            .inner
            .put_multipart(&Path::from(key))
            .await
            .map_err(to_storage_err)?;
        let (result, size) = upload_multipart_file(upload, &mut file)
            .await
            .map_err(to_storage_err)?;
        Ok(ObjectMeta {
            etag: result.e_tag,
            size,
        })
    }

    async fn delete(&self, key: &str) -> Result<()> {
        self.inner
            .delete(&Path::from(key))
            .await
            .map_err(to_storage_err)
    }

    async fn head(&self, key: &str) -> Result<ObjectMeta> {
        let meta = self
            .inner
            .head(&Path::from(key))
            .await
            .map_err(to_storage_err)?;
        Ok(ObjectMeta {
            etag: meta.e_tag,
            size: meta.size,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    use object_store::{MultipartUpload, PutPayload, PutResult, UploadPart};

    use super::upload_multipart_file;

    #[derive(Debug)]
    struct FailingUpload {
        aborted: Arc<AtomicBool>,
    }

    #[derive(Debug)]
    struct PendingUpload {
        part_started: Arc<AtomicBool>,
        aborted: Arc<AtomicBool>,
    }

    #[async_trait::async_trait]
    impl MultipartUpload for PendingUpload {
        fn put_part(&mut self, _data: PutPayload) -> UploadPart {
            self.part_started.store(true, Ordering::SeqCst);
            Box::pin(std::future::pending())
        }

        async fn complete(&mut self) -> object_store::Result<PutResult> {
            unreachable!("pending part 不得 complete")
        }

        async fn abort(&mut self) -> object_store::Result<()> {
            self.aborted.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    #[async_trait::async_trait]
    impl MultipartUpload for FailingUpload {
        fn put_part(&mut self, _data: PutPayload) -> UploadPart {
            Box::pin(async {
                Err(object_store::Error::Generic {
                    store: "test",
                    source: Box::new(std::io::Error::other("part failed")),
                })
            })
        }

        async fn complete(&mut self) -> object_store::Result<PutResult> {
            unreachable!("part 失败后不得 complete")
        }

        async fn abort(&mut self) -> object_store::Result<()> {
            self.aborted.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    #[tokio::test]
    async fn multipart_part_失败会显式_abort() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), b"payload").unwrap();
        let mut file = tokio::fs::File::open(temp.path()).await.unwrap();
        let aborted = Arc::new(AtomicBool::new(false));
        let upload = Box::new(FailingUpload {
            aborted: aborted.clone(),
        });

        assert!(upload_multipart_file(upload, &mut file).await.is_err());
        assert!(aborted.load(Ordering::SeqCst), "失败 multipart 必须 abort");
    }

    #[tokio::test]
    async fn multipart_future_取消后仍会_abort() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), vec![0_u8; super::MULTIPART_CHUNK_SIZE]).unwrap();
        let mut file = tokio::fs::File::open(temp.path()).await.unwrap();
        let part_started = Arc::new(AtomicBool::new(false));
        let aborted = Arc::new(AtomicBool::new(false));
        let upload = Box::new(PendingUpload {
            part_started: part_started.clone(),
            aborted: aborted.clone(),
        });
        let task = tokio::spawn(async move { upload_multipart_file(upload, &mut file).await });
        while !part_started.load(Ordering::SeqCst) {
            tokio::task::yield_now().await;
        }

        task.abort();
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while !aborted.load(Ordering::SeqCst) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("取消 multipart future 后必须异步 abort");
    }
}
