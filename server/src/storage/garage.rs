//! Garage（S3 兼容）实现，基于 `object_store` 的 `AmazonS3`（见 [ADR-0005]）。
//!
//! 委托底层库完成实际 S3 交互，只做类型/错误映射与分页归一，不泄漏其类型给上层。
//!
//! [ADR-0005]: ../../../docs/adr/0005-object-store-over-aws-sdk.md

use std::ops::Range;

use async_trait::async_trait;
use bytes::Bytes;
use futures::StreamExt;
use object_store::aws::{AmazonS3, AmazonS3Builder};
use object_store::path::Path;
use object_store::{ObjectStore as _, ObjectStoreExt as _, PutPayload};

use super::{
    ListEntry, ListPage, ObjectMeta, ObjectStore, Result, StorageError, DEFAULT_PAGE_SIZE,
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
