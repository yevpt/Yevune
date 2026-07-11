//! T4 scanner 集成测试：用内存假 ObjectStore 提供 FLAC fixture，
//! 断言 index 被正确填充、二次扫描 no-op、删除被标记、头部有界读取。

use std::ops::Range;
use std::path::Path;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use music_server::index::Index;
use music_server::scanner::{Scanner, HEADER_READ_CAP};
use music_server::storage::{
    ListPage, MemoryStore, ObjectMeta, ObjectStore, Result as StoreResult,
};
use tempfile::TempDir;
use tokio::sync::Notify;

/// 读取 fixture FLAC 的原始字节。
fn fixture(name: &str) -> Bytes {
    let path = format!(
        "{}/tests/fixtures/scanner/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    Bytes::from(std::fs::read(path).expect("读取 fixture 失败"))
}

/// 打开临时 SQLite 索引。
async fn temp_index() -> (Index, TempDir) {
    let dir = TempDir::new().unwrap();
    let index = Index::connect(&dir.path().join("index.sqlite"))
        .await
        .unwrap();
    (index, dir)
}

/// 装入两首带标签 + 封面的 FLAC 的内存存储。
async fn store_with_two_tracks() -> Arc<MemoryStore> {
    let store = Arc::new(MemoryStore::new());
    store.put("music/a.flac", fixture("a.flac")).await.unwrap();
    store.put("music/b.flac", fixture("b.flac")).await.unwrap();
    store
}

#[tokio::test]
async fn scan_populates_index_from_flac_fixtures() {
    let store = store_with_two_tracks().await;
    let (index, _dir) = temp_index().await;
    let scanner = Scanner::new(store.clone(), index.clone());

    let report = scanner.scan(None).await.unwrap();

    // 两首新增，无更新/删除。
    assert_eq!(report.added, 2, "应新增 2 首");
    assert_eq!(report.updated, 0);
    assert_eq!(report.deleted, 0);

    // 标签写入索引：专辑与曲目可查。
    let albums = index.media().list_albums().await.unwrap();
    let names: Vec<_> = albums.iter().map(|a| a.name.as_str()).collect();
    assert!(names.contains(&"Album A"), "专辑 A 应入库，实际: {names:?}");
    assert!(names.contains(&"Album B"), "专辑 B 应入库，实际: {names:?}");

    // FTS 搜索能命中曲目标题（证明扫描已填充 FTS；精确匹配语义属 index 层测试）。
    let found = index.media().search("Song A", 10).await.unwrap();
    assert!(
        found.tracks.iter().any(|t| t.title == "Song A"),
        "search3 应命中 Song A，实际: {:?}",
        found.tracks.iter().map(|t| &t.title).collect::<Vec<_>>()
    );

    // 抽取的封面单独入库：album.cover_art 指向存在的对象。
    let album_a = albums.iter().find(|a| a.name == "Album A").unwrap();
    let cover_key = album_a.cover_art.as_deref().expect("专辑 A 应有 cover_key");
    let cover = store.get(cover_key).await.unwrap();
    assert!(!cover.is_empty(), "封面对象应非空");
    // 两张专辑封面来源不同（红/蓝），cover_key 应不同。
    let album_b = albums.iter().find(|a| a.name == "Album B").unwrap();
    assert_ne!(album_a.cover_art, album_b.cover_art, "不同封面应有不同 key");
}

#[tokio::test]
async fn second_scan_is_noop_when_unchanged() {
    let store = store_with_two_tracks().await;
    let (index, _dir) = temp_index().await;
    let scanner = Scanner::new(store.clone(), index.clone());

    scanner.scan(None).await.unwrap();
    let again = scanner.scan(None).await.unwrap();

    assert_eq!(again.added, 0, "二次扫描不应新增");
    assert_eq!(again.updated, 0, "etag 未变不应更新");
    assert_eq!(again.deleted, 0);
    assert_eq!(again.unchanged, 2, "两首均应判为未变");

    // 曲目总数保持为 2（未重复入库）。
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tracks")
        .fetch_one(index.pool())
        .await
        .unwrap();
    assert_eq!(count, 2);
}

#[tokio::test]
async fn deleted_object_marks_track_removed() {
    let store = store_with_two_tracks().await;
    let (index, _dir) = temp_index().await;
    let scanner = Scanner::new(store.clone(), index.clone());

    scanner.scan(None).await.unwrap();
    // 源文件消失。
    store.delete("music/b.flac").await.unwrap();
    let report = scanner.scan(None).await.unwrap();

    assert_eq!(report.deleted, 1, "b.flac 消失应删除 1 首");
    assert_eq!(report.unchanged, 1, "a.flac 仍在应为未变");

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tracks")
        .fetch_one(index.pool())
        .await
        .unwrap();
    assert_eq!(count, 1, "应只剩 a.flac");
    let gone: Option<i64> =
        sqlx::query_scalar("SELECT id FROM tracks WHERE object_key = 'music/b.flac'")
            .fetch_optional(index.pool())
            .await
            .unwrap();
    assert!(gone.is_none(), "b.flac 曲目应已删除");
}

#[tokio::test]
async fn etag_change_updates_track_in_place() {
    let store = Arc::new(MemoryStore::new());
    store.put("music/a.flac", fixture("a.flac")).await.unwrap();
    let (index, _dir) = temp_index().await;
    let scanner = Scanner::new(store.clone(), index.clone());

    scanner.scan(None).await.unwrap();
    let id_before: i64 =
        sqlx::query_scalar("SELECT id FROM tracks WHERE object_key = 'music/a.flac'")
            .fetch_one(index.pool())
            .await
            .unwrap();

    // 同一 key 覆盖为不同内容（b.flac 标签/etag 不同）。
    store.put("music/a.flac", fixture("b.flac")).await.unwrap();
    let report = scanner.scan(None).await.unwrap();

    assert_eq!(report.updated, 1, "etag 变化应更新 1 首");
    assert_eq!(report.added, 0);
    assert_eq!(report.deleted, 0);

    // 原地更新：主键不变，标题变为 b 的标题。
    let (id_after, title): (i64, String) =
        sqlx::query_as("SELECT id, title FROM tracks WHERE object_key = 'music/a.flac'")
            .fetch_one(index.pool())
            .await
            .unwrap();
    assert_eq!(id_after, id_before, "应原地更新，主键不变");
    assert_eq!(title, "Song B", "标题应更新为新内容");
}

/// 记录 `get_range` 请求过的最大结束偏移的包装存储，用于验证「只读文件头」红线。
struct CountingStore {
    inner: MemoryStore,
    max_end: AtomicU64,
}

impl CountingStore {
    fn new() -> Self {
        Self {
            inner: MemoryStore::new(),
            max_end: AtomicU64::new(0),
        }
    }
}

#[async_trait]
impl ObjectStore for CountingStore {
    async fn list(&self, prefix: &str, token: Option<String>) -> StoreResult<ListPage> {
        self.inner.list(prefix, token).await
    }
    async fn get(&self, key: &str) -> StoreResult<Bytes> {
        self.inner.get(key).await
    }
    async fn get_range(&self, key: &str, range: Range<u64>) -> StoreResult<Bytes> {
        self.max_end.fetch_max(range.end, Ordering::SeqCst);
        self.inner.get_range(key, range).await
    }
    async fn put(&self, key: &str, bytes: Bytes) -> StoreResult<ObjectMeta> {
        self.inner.put(key, bytes).await
    }
    async fn put_file(&self, key: &str, path: &Path) -> StoreResult<ObjectMeta> {
        self.inner.put_file(key, path).await
    }
    async fn delete(&self, key: &str) -> StoreResult<()> {
        self.inner.delete(key).await
    }
    async fn head(&self, key: &str) -> StoreResult<ObjectMeta> {
        self.inner.head(key).await
    }
}

#[tokio::test]
async fn scan_reads_only_header_not_whole_file() {
    let big = fixture("large.flac");
    let file_size = big.len() as u64;
    assert!(
        file_size > HEADER_READ_CAP,
        "fixture 需大于头部上限才能验证有界读取（{file_size} <= {HEADER_READ_CAP}）"
    );

    let store = Arc::new(CountingStore::new());
    store.put("music/large.flac", big).await.unwrap();
    let (index, _dir) = temp_index().await;
    let scanner = Scanner::new(store.clone(), index.clone());

    let report = scanner.scan(None).await.unwrap();
    assert_eq!(report.added, 1, "大文件应入库");

    // 红线：读取的最大偏移不得超过头部上限，更不得达到整文件大小。
    let max_end = store.max_end.load(Ordering::SeqCst);
    assert!(
        max_end <= HEADER_READ_CAP,
        "读取偏移 {max_end} 超过头部上限 {HEADER_READ_CAP}"
    );
    assert!(max_end < file_size, "绝不应整读音频文件");

    // 且标签仍被正确解析。
    let title: String =
        sqlx::query_scalar("SELECT title FROM tracks WHERE object_key = 'music/large.flac'")
            .fetch_one(index.pool())
            .await
            .unwrap();
    assert_eq!(title, "Big Song");
}

#[tokio::test]
async fn scan_status_reports_completion() {
    let store = store_with_two_tracks().await;
    let (index, _dir) = temp_index().await;
    let scanner = Scanner::new(store.clone(), index.clone());

    // 扫描前：未开始、无完成时间。
    let before = scanner.scan_status();
    assert!(!before.scanning);
    assert!(before.last_scan_at.is_none());

    scanner.scan(None).await.unwrap();

    let after = scanner.scan_status();
    assert!(!after.scanning, "扫描结束应复位 scanning");
    assert!(after.last_scan_at.is_some(), "完成后应记录 last_scan_at");
    assert!(after.error.is_none());
    assert_eq!(after.scanned, 2, "应统计已处理对象数");
}

struct BlockingListStore {
    inner: MemoryStore,
    list_calls: AtomicUsize,
    entered: Notify,
    release: Notify,
}

impl BlockingListStore {
    fn new() -> Self {
        Self {
            inner: MemoryStore::new(),
            list_calls: AtomicUsize::new(0),
            entered: Notify::new(),
            release: Notify::new(),
        }
    }
}

#[async_trait]
impl ObjectStore for BlockingListStore {
    async fn list(&self, prefix: &str, token: Option<String>) -> StoreResult<ListPage> {
        self.list_calls.fetch_add(1, Ordering::SeqCst);
        self.entered.notify_one();
        self.release.notified().await;
        self.inner.list(prefix, token).await
    }

    async fn get(&self, key: &str) -> StoreResult<Bytes> {
        self.inner.get(key).await
    }

    async fn get_range(&self, key: &str, range: Range<u64>) -> StoreResult<Bytes> {
        self.inner.get_range(key, range).await
    }

    async fn put(&self, key: &str, bytes: Bytes) -> StoreResult<ObjectMeta> {
        self.inner.put(key, bytes).await
    }

    async fn put_file(&self, key: &str, path: &Path) -> StoreResult<ObjectMeta> {
        self.inner.put_file(key, path).await
    }

    async fn delete(&self, key: &str) -> StoreResult<()> {
        self.inner.delete(key).await
    }

    async fn head(&self, key: &str) -> StoreResult<ObjectMeta> {
        self.inner.head(key).await
    }
}

#[tokio::test]
async fn background_scan_start_is_single_flight() {
    let store = Arc::new(BlockingListStore::new());
    let (index, _dir) = temp_index().await;
    let scanner = Arc::new(Scanner::new(store.clone(), index));

    assert!(scanner.try_start(None), "首次后台扫描应启动");
    tokio::time::timeout(std::time::Duration::from_secs(1), store.entered.notified())
        .await
        .expect("后台扫描应进入对象列举");
    assert!(scanner.scan_status().scanning);
    assert!(!scanner.try_start(None), "运行期间的重复启动必须被拒绝");
    assert_eq!(store.list_calls.load(Ordering::SeqCst), 1);

    store.release.notify_one();
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while scanner.scan_status().scanning {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("后台扫描应完成并释放 single-flight 状态");
}

#[tokio::test]
async fn cancelled_scan_releases_single_flight_state() {
    let store = Arc::new(BlockingListStore::new());
    let (index, _dir) = temp_index().await;
    let scanner = Arc::new(Scanner::new(store.clone(), index));
    let running = {
        let scanner = scanner.clone();
        tokio::spawn(async move { scanner.scan(None).await })
    };
    tokio::time::timeout(std::time::Duration::from_secs(1), store.entered.notified())
        .await
        .expect("扫描应进入对象列举");
    running.abort();
    let _ = running.await;

    assert!(
        !scanner.scan_status().scanning,
        "取消扫描必须复位 single-flight"
    );
    assert!(scanner.try_start(None), "取消后应能重新启动扫描");
    store.release.notify_one();
}
