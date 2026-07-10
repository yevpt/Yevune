//! 内存假实现 [`MemoryStore`]：供 storage 层及下游（scanner/transcode）单测替换真实存储。
//!
//! 用 `BTreeMap` 保证 key 升序，贴合 S3 列举语义；ETag 由内容哈希派生（确定性、随内容变化）。

use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::sync::Mutex;

use async_trait::async_trait;
use bytes::Bytes;

use super::{
    ListEntry, ListPage, ObjectMeta, ObjectStore, Result, StorageError, DEFAULT_PAGE_SIZE,
};

/// 单个存储对象。
#[derive(Clone)]
struct StoredObject {
    data: Bytes,
    etag: String,
}

/// 进程内内存对象存储（假实现）。
pub struct MemoryStore {
    objects: Mutex<BTreeMap<String, StoredObject>>,
    page_size: usize,
}

impl MemoryStore {
    /// 用默认页大小创建空存储。
    pub fn new() -> Self {
        Self::with_page_size(DEFAULT_PAGE_SIZE)
    }

    /// 用指定页大小创建空存储（便于测试分页）。
    ///
    /// # Panics
    /// `page_size` 为 0 时 panic。
    pub fn with_page_size(page_size: usize) -> Self {
        assert!(page_size > 0, "page_size 必须为正");
        Self {
            objects: Mutex::new(BTreeMap::new()),
            page_size,
        }
    }
}

impl Default for MemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

/// 由内容派生确定性 ETag（假实现用；真实 S3 为 MD5）。
fn etag_of(data: &[u8]) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    data.hash(&mut h);
    format!("{:016x}", h.finish())
}

#[async_trait]
impl ObjectStore for MemoryStore {
    async fn list(&self, prefix: &str, token: Option<String>) -> Result<ListPage> {
        let objects = self.objects.lock().unwrap();
        let mut entries = Vec::new();
        let mut next_token = None;

        let matching = objects.iter().filter(|(k, _)| {
            k.starts_with(prefix) && token.as_deref().is_none_or(|t| k.as_str() > t)
        });
        for (key, obj) in matching {
            if entries.len() == self.page_size {
                // 已装满一页且仍有后续匹配项 → 需要续页。
                next_token = entries.last().map(|e: &ListEntry| e.key.clone());
                break;
            }
            entries.push(ListEntry {
                key: key.clone(),
                etag: Some(obj.etag.clone()),
                size: obj.data.len() as u64,
            });
        }

        Ok(ListPage {
            entries,
            next_token,
        })
    }

    async fn get(&self, key: &str) -> Result<Bytes> {
        let objects = self.objects.lock().unwrap();
        objects
            .get(key)
            .map(|o| o.data.clone())
            .ok_or_else(|| StorageError::NotFound(key.to_string()))
    }

    async fn get_range(&self, key: &str, range: Range<u64>) -> Result<Bytes> {
        let objects = self.objects.lock().unwrap();
        let obj = objects
            .get(key)
            .ok_or_else(|| StorageError::NotFound(key.to_string()))?;
        let len = obj.data.len() as u64;
        let start = range.start.min(len) as usize;
        let end = range.end.min(len) as usize;
        Ok(obj.data.slice(start..end))
    }

    async fn put(&self, key: &str, bytes: Bytes) -> Result<ObjectMeta> {
        let etag = etag_of(&bytes);
        let size = bytes.len() as u64;
        let mut objects = self.objects.lock().unwrap();
        objects.insert(
            key.to_string(),
            StoredObject {
                data: bytes,
                etag: etag.clone(),
            },
        );
        Ok(ObjectMeta {
            etag: Some(etag),
            size,
        })
    }

    async fn delete(&self, key: &str) -> Result<()> {
        // 不存在视为成功（幂等）。
        self.objects.lock().unwrap().remove(key);
        Ok(())
    }

    async fn head(&self, key: &str) -> Result<ObjectMeta> {
        let objects = self.objects.lock().unwrap();
        objects
            .get(key)
            .map(|o| ObjectMeta {
                etag: Some(o.etag.clone()),
                size: o.data.len() as u64,
            })
            .ok_or_else(|| StorageError::NotFound(key.to_string()))
    }
}
