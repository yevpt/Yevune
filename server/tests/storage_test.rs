//! storage 层单元测试：针对内存假实现 [`MemoryStore`] 验证 `ObjectStore` 契约。
//!
//! 假实现与 Garage 真实现共用同一 trait，故这些测试同时锁定 trait 的行为语义，
//! 也供下游（scanner/transcode）测试直接复用假实现。

use bytes::Bytes;
use yevune_server::storage::{MemoryStore, ObjectStore, StorageError, STREAM_CHUNK_SIZE};

fn payload(s: &str) -> Bytes {
    Bytes::from(s.as_bytes().to_vec())
}

#[tokio::test]
async fn put_file_以有界分块上传完整文件() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cache.bin");
    let bytes = vec![0x5a; STREAM_CHUNK_SIZE * 2 + 17];
    std::fs::write(&path, &bytes).unwrap();
    let store = MemoryStore::new();

    let meta = store
        .put_file("transcode/1/opus_128.opus", &path)
        .await
        .unwrap();

    assert_eq!(meta.size, bytes.len() as u64);
    assert_eq!(
        store.get("transcode/1/opus_128.opus").await.unwrap(),
        Bytes::from(bytes)
    );
}

#[tokio::test]
async fn put_然后_head_返回_size_与_etag() {
    let store = MemoryStore::new();
    let meta = store.put("library/a.flac", payload("hello")).await.unwrap();
    assert_eq!(meta.size, 5);
    assert!(meta.etag.is_some(), "put 应返回 etag");

    let head = store.head("library/a.flac").await.unwrap();
    assert_eq!(head.size, 5);
    assert_eq!(head.etag, meta.etag, "head 的 etag 应与 put 一致");
}

#[tokio::test]
async fn get_返回完整字节() {
    let store = MemoryStore::new();
    store.put("k", payload("abcdef")).await.unwrap();
    let got = store.get("k").await.unwrap();
    assert_eq!(&got[..], b"abcdef");
}

#[tokio::test]
async fn get_range_返回半开区间子串() {
    let store = MemoryStore::new();
    store.put("k", payload("0123456789")).await.unwrap();
    // [2, 5) → "234"
    let got = store.get_range("k", 2..5).await.unwrap();
    assert_eq!(&got[..], b"234");
}

#[tokio::test]
async fn head_不存在的_key_报_not_found() {
    let store = MemoryStore::new();
    let err = store.head("missing").await.unwrap_err();
    assert!(
        matches!(err, StorageError::NotFound(_)),
        "应为 NotFound，实际：{err:?}"
    );
}

#[tokio::test]
async fn get_不存在的_key_报_not_found() {
    let store = MemoryStore::new();
    let err = store.get("missing").await.unwrap_err();
    assert!(
        matches!(err, StorageError::NotFound(_)),
        "应为 NotFound，实际：{err:?}"
    );
}

#[tokio::test]
async fn delete_后_head_报_not_found() {
    let store = MemoryStore::new();
    store.put("k", payload("x")).await.unwrap();
    store.head("k").await.unwrap();
    store.delete("k").await.unwrap();
    let err = store.head("k").await.unwrap_err();
    assert!(matches!(err, StorageError::NotFound(_)));
}

#[tokio::test]
async fn delete_不存在的_key_幂等() {
    let store = MemoryStore::new();
    // 删除不存在的对象不应报错（幂等），与 S3 语义一致。
    store.delete("nope").await.unwrap();
}

#[tokio::test]
async fn list_按前缀过滤_并按_key_升序() {
    let store = MemoryStore::new();
    store.put("library/b.flac", payload("2")).await.unwrap();
    store.put("library/a.flac", payload("1")).await.unwrap();
    store.put("covers/x.jpg", payload("3")).await.unwrap();

    let page = store.list("library/", None).await.unwrap();
    let keys: Vec<_> = page.entries.iter().map(|e| e.key.clone()).collect();
    assert_eq!(keys, vec!["library/a.flac", "library/b.flac"]);
    assert!(page.next_token.is_none(), "结果未满一页，不应有续页 token");
    // 元数据随列举一并返回。
    assert_eq!(page.entries[0].size, 1);
    assert!(page.entries[0].etag.is_some());
}

#[tokio::test]
async fn list_空前缀列举全部() {
    let store = MemoryStore::new();
    store.put("a", payload("1")).await.unwrap();
    store.put("b", payload("2")).await.unwrap();
    let page = store.list("", None).await.unwrap();
    assert_eq!(page.entries.len(), 2);
}

#[tokio::test]
async fn list_分页_用_token_翻页且不重不漏() {
    // 页大小设为 2，插入 5 个 key，验证跨页遍历完整且有序。
    let store = MemoryStore::with_page_size(2);
    for k in ["k1", "k2", "k3", "k4", "k5"] {
        store.put(k, payload(k)).await.unwrap();
    }

    let mut seen = Vec::new();
    let mut token = None;
    loop {
        let page = store.list("k", token).await.unwrap();
        assert!(page.entries.len() <= 2, "每页不超过页大小");
        seen.extend(page.entries.iter().map(|e| e.key.clone()));
        match page.next_token {
            Some(t) => token = Some(t),
            None => break,
        }
    }
    assert_eq!(
        seen,
        vec!["k1", "k2", "k3", "k4", "k5"],
        "跨页遍历应完整有序、不重不漏"
    );
}
