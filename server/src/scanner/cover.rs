//! 内嵌封面入库：把从文件头抽出的封面单独 `put` 回对象存储，返回其 `cover_key`。
//!
//! key 由封面内容的 SHA-256 派生（`covers/{hash}.{ext}`）：相同封面天然去重，
//! 重复 `put` 幂等，且不与音频 key 冲突（扫描时按扩展名过滤，不会把封面当音频入库）。

use sha2::{Digest, Sha256};

use crate::storage::{ObjectStore, StorageError};

use super::tags::ParsedCover;

/// 由封面内容派生稳定去重的对象 key。
pub fn cover_key(cover: &ParsedCover) -> String {
    let digest = hex::encode(Sha256::digest(&cover.data));
    let ext = ext_for_mime(cover.mime.as_deref());
    format!("covers/{digest}.{ext}")
}

/// 把封面写入存储，返回其 key（内容寻址 → 幂等）。
pub async fn store_cover(
    store: &dyn ObjectStore,
    cover: &ParsedCover,
) -> Result<String, StorageError> {
    let key = cover_key(cover);
    store.put(&key, cover.data.clone()).await?;
    Ok(key)
}

/// MIME → 文件扩展名。
fn ext_for_mime(mime: Option<&str>) -> &'static str {
    match mime {
        Some("image/jpeg") | Some("image/jpg") => "jpg",
        Some("image/png") => "png",
        Some("image/gif") => "gif",
        Some("image/webp") => "webp",
        Some("image/bmp") => "bmp",
        _ => "img",
    }
}
