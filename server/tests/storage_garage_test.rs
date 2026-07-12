//! storage 层集成测试：针对真实 S3 兼容后端（本地 MinIO/Garage）验证 [`GarageStore`]。
//!
//! 通过环境变量注入连接信息；**未配置端点时自动跳过**，以保 CI（无后端）绿。
//! 本地验证：起一个 MinIO/Garage 后设置下列变量再 `cargo test`：
//!
//! ```text
//! YEVUNE_TEST_S3_ENDPOINT=http://127.0.0.1:9000
//! YEVUNE_TEST_S3_BUCKET=yevune
//! YEVUNE_TEST_S3_ACCESS_KEY=...
//! YEVUNE_TEST_S3_SECRET_KEY=...
//! ```

use bytes::Bytes;
use yevune_server::storage::{GarageConfig, GarageStore, ObjectStore, StorageError};

/// 从环境变量构造配置；未配置端点时返回 None（调用方跳过）。
fn config_from_env() -> Option<GarageConfig> {
    let endpoint = std::env::var("YEVUNE_TEST_S3_ENDPOINT").ok()?;
    let bucket = std::env::var("YEVUNE_TEST_S3_BUCKET").unwrap_or_else(|_| "yevune".to_string());
    let access_key = std::env::var("YEVUNE_TEST_S3_ACCESS_KEY").unwrap_or_default();
    let secret_key = std::env::var("YEVUNE_TEST_S3_SECRET_KEY").unwrap_or_default();
    Some(GarageConfig::new(endpoint, bucket, access_key, secret_key))
}

/// 从环境变量构造 GarageStore；未配置端点时返回 None（调用方跳过）。
fn store_from_env() -> Option<GarageStore> {
    let config = config_from_env()?;
    Some(GarageStore::new(config).expect("构造 GarageStore 失败"))
}

#[tokio::test]
async fn garage_往返_put_head_get_range_list_delete() {
    let Some(store) = store_from_env() else {
        eprintln!("跳过：未设置 YEVUNE_TEST_S3_ENDPOINT（无本地 S3 后端）");
        return;
    };

    // 每次运行用独立前缀，避免与并发/历史数据冲突。
    let prefix = format!("it-{}/", std::process::id());
    let key = format!("{prefix}song.bin");
    let body = Bytes::from_static(b"0123456789abcdef");

    // put → 返回 etag 与 size。
    let put_meta = store.put(&key, body.clone()).await.expect("put 失败");
    assert_eq!(put_meta.size, body.len() as u64);

    // head → etag/size 与写入一致。
    let head = store.head(&key).await.expect("head 失败");
    assert_eq!(head.size, body.len() as u64);
    assert!(head.etag.is_some(), "head 应返回 etag");

    // get_range → 取 [4, 8) = "4567"。
    let ranged = store.get_range(&key, 4..8).await.expect("get_range 失败");
    assert_eq!(&ranged[..], b"4567");

    // get → 完整内容。
    let full = store.get(&key).await.expect("get 失败");
    assert_eq!(&full[..], &body[..]);

    // list → 该前缀下含刚写入的 key。
    let page = store.list(&prefix, None).await.expect("list 失败");
    assert!(
        page.entries.iter().any(|e| e.key == key),
        "list 应含刚写入的 key，实际：{:?}",
        page.entries
    );

    // delete → 之后 head 报 NotFound。
    store.delete(&key).await.expect("delete 失败");
    let err = store.head(&key).await.unwrap_err();
    assert!(
        matches!(err, StorageError::NotFound(_)),
        "删除后应 NotFound，实际：{err:?}"
    );
}

#[tokio::test]
async fn garage_分页_用_token_翻页且不重不漏() {
    let Some(mut config) = config_from_env() else {
        eprintln!("跳过：未设置 YEVUNE_TEST_S3_ENDPOINT（无本地 S3 后端）");
        return;
    };
    // 小页大小以真实触发 list_with_offset 续页路径。
    config.page_size = 2;
    let store = GarageStore::new(config).expect("构造 GarageStore 失败");

    let prefix = format!("pg-{}/", std::process::id());
    let keys: Vec<String> = (1..=5).map(|i| format!("{prefix}k{i}")).collect();
    for k in &keys {
        store
            .put(k, Bytes::from_static(b"x"))
            .await
            .expect("put 失败");
    }

    let mut seen = Vec::new();
    let mut token = None;
    loop {
        let page = store.list(&prefix, token).await.expect("list 失败");
        assert!(page.entries.len() <= 2, "每页不超过页大小");
        seen.extend(page.entries.iter().map(|e| e.key.clone()));
        match page.next_token {
            Some(t) => token = Some(t),
            None => break,
        }
    }
    assert_eq!(seen, keys, "跨页遍历应完整有序、不重不漏");

    // 清理。
    for k in &keys {
        store.delete(k).await.expect("delete 失败");
    }
}

#[tokio::test]
async fn garage_put_file_大于单_part_可往返并删除() {
    let Some(store) = store_from_env() else {
        eprintln!("跳过：未设置 YEVUNE_TEST_S3_ENDPOINT（无本地 S3 后端）");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("multipart.bin");
    let size = 5 * 1024 * 1024 + 123;
    let body = vec![0x6d_u8; size];
    std::fs::write(&path, &body).unwrap();
    let key = format!("multipart-{}/large.bin", std::process::id());

    let put = store.put_file(&key, &path).await.expect("put_file 失败");
    assert_eq!(put.size, size as u64);
    assert_eq!(store.head(&key).await.unwrap().size, size as u64);
    let boundary = store
        .get_range(&key, 5 * 1024 * 1024 - 2..5 * 1024 * 1024 + 2)
        .await
        .unwrap();
    assert_eq!(boundary, Bytes::from_static(&[0x6d; 4]));
    store.delete(&key).await.unwrap();
    assert!(matches!(
        store.head(&key).await,
        Err(StorageError::NotFound(_))
    ));
}
