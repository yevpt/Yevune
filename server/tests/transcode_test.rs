//! T5 FFmpeg 按需转码与缓存集成测试。

use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use bytes::{Bytes, BytesMut};
use futures::StreamExt;
use yevune_server::index::{Index, NewTrack};
use yevune_server::storage::{
    ListPage, MemoryStore, ObjectMeta, ObjectStore, Result as StoreResult,
};
use yevune_server::transcode::{
    should_transcode, ByteStream, TranscodeTarget, TranscodeTrack, Transcoder,
};

fn fixture(name: &str) -> Bytes {
    let path = format!(
        "{}/tests/fixtures/scanner/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    Bytes::from(std::fs::read(path).expect("读取 fixture 失败"))
}

async fn temp_index() -> (Index, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let index = Index::connect(&dir.path().join("index.sqlite"))
        .await
        .unwrap();
    (index, dir)
}

async fn insert_track(
    index: &Index,
    store: &MemoryStore,
    key: &str,
    bytes: Bytes,
) -> TranscodeTrack {
    let size = bytes.len() as u64;
    store.put(key, bytes).await.unwrap();
    let id = index
        .media()
        .upsert_track(&NewTrack {
            title: key.into(),
            duration: Some(10),
            codec: Some("flac".into()),
            bitrate: Some(800),
            size: Some(size),
            object_key: key.into(),
            ..Default::default()
        })
        .await
        .unwrap();
    TranscodeTrack::new(id, key, "flac", 800)
}

async fn collect(mut stream: ByteStream) -> Result<Bytes, yevune_server::transcode::Error> {
    let mut output = BytesMut::new();
    while let Some(chunk) = stream.next().await {
        output.extend_from_slice(&chunk?);
    }
    Ok(output.freeze())
}

fn opus_96() -> TranscodeTarget {
    TranscodeTarget::new("opus", 96)
}

fn aac_128() -> TranscodeTarget {
    TranscodeTarget::new("aac", 128)
}

#[test]
fn passthrough_decision_handles_raw_compatible_and_bitrate_cap() {
    let flac = TranscodeTrack::new(1, "library/a.flac", "flac", 800);
    assert!(!should_transcode(&flac, &TranscodeTarget::new("raw", 0)));
    assert!(!should_transcode(
        &flac,
        &TranscodeTarget::new("flac", 1000)
    ));
    assert!(should_transcode(&flac, &TranscodeTarget::new("flac", 320)));
    assert!(should_transcode(&flac, &opus_96()));
}

#[tokio::test]
async fn original_compatible_track_is_streamed_without_ffmpeg() {
    let (index, _dir) = temp_index().await;
    let store = Arc::new(MemoryStore::new());
    let source = fixture("a.flac");
    let track = insert_track(&index, &store, "library/a.flac", source.clone()).await;
    let transcoder = Transcoder::new(store, index, PathBuf::from("definitely-no-ffmpeg"));

    let output = collect(
        transcoder
            .stream(track, TranscodeTarget::new("raw", 0))
            .await
            .unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(output, source);
}

#[tokio::test]
async fn first_request_transcodes_playable_output_and_registers_cache() {
    let (index, dir) = temp_index().await;
    let store = Arc::new(MemoryStore::new());
    let track = insert_track(&index, &store, "library/a.flac", fixture("a.flac")).await;
    let track_id = track.id;
    let transcoder = Transcoder::new(store.clone(), index.clone(), PathBuf::from("ffmpeg"));

    let output = collect(transcoder.stream(track, opus_96()).await.unwrap())
        .await
        .unwrap();

    assert!(output.starts_with(b"OggS"), "Opus 输出应为 Ogg 容器");
    let output_path = dir.path().join("output.opus");
    std::fs::write(&output_path, &output).unwrap();
    assert_playable(&output_path);
    let expected_key = format!("transcode/{track_id}/opus_96.opus");
    let cached = index
        .transcode_cache()
        .get(track_id, "opus", 96)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(cached.object_key, expected_key);
    assert_eq!(cached.size, output.len() as u64);
    assert_eq!(store.get(&expected_key).await.unwrap(), output);
}

#[tokio::test]
async fn second_request_streams_cache_without_source_or_ffmpeg() {
    let (index, _dir) = temp_index().await;
    let store = Arc::new(MemoryStore::new());
    let track = insert_track(&index, &store, "library/a.flac", fixture("a.flac")).await;
    let first = Transcoder::new(store.clone(), index.clone(), PathBuf::from("ffmpeg"));
    let expected = collect(first.stream(track.clone(), opus_96()).await.unwrap())
        .await
        .unwrap();
    store.delete(&track.object_key).await.unwrap();
    let cached_only = Transcoder::new(
        store,
        index,
        PathBuf::from("definitely-no-ffmpeg-on-cache-hit"),
    );

    let actual = collect(cached_only.stream(track, opus_96()).await.unwrap())
        .await
        .unwrap();

    assert_eq!(actual, expected);
}

#[tokio::test]
async fn aac_output_is_playable_and_second_request_hits_cache() {
    let (index, dir) = temp_index().await;
    let store = Arc::new(MemoryStore::new());
    let track = insert_track(&index, &store, "library/a.flac", fixture("a.flac")).await;
    let first = Transcoder::new(store.clone(), index.clone(), PathBuf::from("ffmpeg"));

    let expected = collect(first.stream(track.clone(), aac_128()).await.unwrap())
        .await
        .unwrap();

    assert_eq!(expected[0], 0xff, "AAC/ADTS 应以同步字开头");
    assert_eq!(expected[1] & 0xf0, 0xf0, "AAC/ADTS 同步字应完整");
    let output_path = dir.path().join("output.aac");
    std::fs::write(&output_path, &expected).unwrap();
    assert_playable(&output_path);
    store.delete(&track.object_key).await.unwrap();
    let cached_only = Transcoder::new(
        store,
        index,
        PathBuf::from("definitely-no-ffmpeg-on-aac-cache-hit"),
    );
    let actual = collect(cached_only.stream(track, aac_128()).await.unwrap())
        .await
        .unwrap();
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn interrupted_consumer_leaves_no_cache_object_or_row() {
    let (index, _dir) = temp_index().await;
    let store = Arc::new(MemoryStore::new());
    let track = insert_track(&index, &store, "library/large.flac", fixture("large.flac")).await;
    let id = track.id;
    let transcoder = Transcoder::new(store.clone(), index.clone(), PathBuf::from("ffmpeg"));
    let mut stream = transcoder.stream(track, opus_96()).await.unwrap();

    let first = stream.next().await.unwrap().unwrap();
    assert!(!first.is_empty());
    drop(stream);

    wait_for_cleanup(&index, store.as_ref(), id).await;
    assert!(index
        .transcode_cache()
        .get(id, "opus", 96)
        .await
        .unwrap()
        .is_none());
    assert!(store
        .list(&format!("transcode/{id}/"), None)
        .await
        .unwrap()
        .entries
        .is_empty());
}

#[tokio::test]
async fn ffmpeg_failure_leaves_no_cache_object_or_row() {
    let (index, _dir) = temp_index().await;
    let store = Arc::new(MemoryStore::new());
    let track = insert_track(
        &index,
        &store,
        "library/broken.flac",
        Bytes::from_static(b"not audio"),
    )
    .await;
    let id = track.id;
    let transcoder = Transcoder::new(store.clone(), index.clone(), PathBuf::from("ffmpeg"));

    let result = collect(transcoder.stream(track, opus_96()).await.unwrap()).await;

    assert!(result.is_err(), "损坏输入应使 FFmpeg 失败");
    assert!(index
        .transcode_cache()
        .get(id, "opus", 96)
        .await
        .unwrap()
        .is_none());
    assert!(store
        .list(&format!("transcode/{id}/"), None)
        .await
        .unwrap()
        .entries
        .is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn ffmpeg_processes_are_limited_by_semaphore() {
    use std::os::unix::fs::PermissionsExt;

    let (index, _db_dir) = temp_index().await;
    let state = tempfile::tempdir().unwrap();
    let script = state.path().join("fake-ffmpeg.sh");
    let script_body = format!(
        "#!/bin/sh\nif ! mkdir '{0}/lock' 2>/dev/null; then touch '{0}/overlap'; fi\necho start >> '{0}/starts'\nsleep 0.2\ncat\nrmdir '{0}/lock' 2>/dev/null\n",
        state.path().display()
    );
    std::fs::write(&script, script_body).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let store = Arc::new(MemoryStore::new());
    let one = insert_track(&index, &store, "library/one.flac", fixture("a.flac")).await;
    let two = insert_track(&index, &store, "library/two.flac", fixture("b.flac")).await;
    let transcoder = Transcoder::with_concurrency(store, index, script, 1);

    let one_stream = transcoder.stream(one, opus_96()).await.unwrap();
    let two_stream = transcoder.stream(two, opus_96()).await.unwrap();
    let (one_result, two_result) = tokio::join!(collect(one_stream), collect(two_stream));

    one_result.unwrap();
    two_result.unwrap();
    assert!(
        !state.path().join("overlap").exists(),
        "FFmpeg 子进程发生并发重叠"
    );
    assert_eq!(
        std::fs::read_to_string(state.path().join("starts"))
            .unwrap()
            .lines()
            .count(),
        2
    );
}

#[cfg(unix)]
#[tokio::test]
async fn interrupted_consumer_terminates_stalled_ffmpeg_process() {
    use std::os::unix::fs::PermissionsExt;

    let (index, _db_dir) = temp_index().await;
    let state = tempfile::tempdir().unwrap();
    let script = state.path().join("stalled-ffmpeg.sh");
    let script_body = format!(
        "#!/bin/sh\necho $$ > '{0}/pid'\nprintf chunk\nwhile :; do sleep 1; done\n",
        state.path().display()
    );
    std::fs::write(&script, script_body).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let store = Arc::new(MemoryStore::new());
    let track = insert_track(&index, &store, "library/stalled.flac", fixture("a.flac")).await;
    let transcoder = Transcoder::with_concurrency(store, index, script, 1);
    let mut stream = transcoder.stream(track, opus_96()).await.unwrap();

    assert_eq!(
        stream.next().await.unwrap().unwrap(),
        Bytes::from_static(b"chunk")
    );
    let pid: u32 = std::fs::read_to_string(state.path().join("pid"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    drop(stream);

    tokio::time::timeout(Duration::from_secs(2), async {
        while process_alive(pid) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("调用方中断后 FFmpeg 子进程应及时终止");
}

#[cfg(unix)]
#[tokio::test]
async fn consumer_drop_after_stdout_eof_terminates_hung_ffmpeg() {
    use std::os::unix::fs::PermissionsExt;

    let (index, _db_dir) = temp_index().await;
    let state = tempfile::tempdir().unwrap();
    let script = state.path().join("eof-then-hang-ffmpeg.sh");
    let script_body = format!(
        "#!/bin/sh\necho $$ > '{0}/pid'\nprintf tail\nexec 1>&-\nwhile :; do sleep 1; done\n",
        state.path().display()
    );
    std::fs::write(&script, script_body).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let store = Arc::new(MemoryStore::new());
    let track = insert_track(&index, &store, "library/eof.flac", fixture("a.flac")).await;
    let id = track.id;
    let transcoder = Transcoder::with_concurrency(store.clone(), index.clone(), script, 1);
    let mut stream = transcoder.stream(track, opus_96()).await.unwrap();

    assert_eq!(
        stream.next().await.unwrap().unwrap(),
        Bytes::from_static(b"tail")
    );
    let pid: u32 = std::fs::read_to_string(state.path().join("pid"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    drop(stream);

    tokio::time::timeout(Duration::from_secs(2), async {
        while process_alive(pid) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("stdout EOF 后调用方中断仍应终止 FFmpeg");
    assert!(index
        .transcode_cache()
        .get(id, "opus", 96)
        .await
        .unwrap()
        .is_none());
    assert!(store
        .list(&format!("transcode/{id}/"), None)
        .await
        .unwrap()
        .entries
        .is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn last_chunk_without_observing_eof_is_not_cached() {
    use std::os::unix::fs::PermissionsExt;

    let (index, _db_dir) = temp_index().await;
    let state = tempfile::tempdir().unwrap();
    let script = state.path().join("one-chunk-ffmpeg.sh");
    std::fs::write(&script, "#!/bin/sh\nprintf complete\n").unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let store = Arc::new(MemoryStore::new());
    let track = insert_track(&index, &store, "library/last.flac", fixture("a.flac")).await;
    let id = track.id;
    let transcoder = Transcoder::with_concurrency(store.clone(), index.clone(), script, 1);
    let mut stream = transcoder.stream(track, opus_96()).await.unwrap();

    assert_eq!(
        stream.next().await.unwrap().unwrap(),
        Bytes::from_static(b"complete")
    );
    tokio::time::sleep(Duration::from_millis(200)).await;

    assert!(index
        .transcode_cache()
        .get(id, "opus", 96)
        .await
        .unwrap()
        .is_none());
    assert!(store
        .list(&format!("transcode/{id}/"), None)
        .await
        .unwrap()
        .entries
        .is_empty());
    drop(stream);
}

#[cfg(unix)]
#[tokio::test]
async fn concurrent_same_cache_key_runs_ffmpeg_once() {
    use std::os::unix::fs::PermissionsExt;

    let (index, _db_dir) = temp_index().await;
    let state = tempfile::tempdir().unwrap();
    let script = state.path().join("counting-ffmpeg.sh");
    let script_body = format!(
        "#!/bin/sh\necho start >> '{0}/starts'\ncat\n",
        state.path().display()
    );
    std::fs::write(&script, script_body).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let store = Arc::new(MemoryStore::new());
    let track = insert_track(&index, &store, "library/shared.flac", fixture("a.flac")).await;
    let transcoder = Transcoder::with_concurrency(store, index, script, 2);

    let first = transcoder.stream(track.clone(), opus_96()).await.unwrap();
    let second = transcoder.stream(track, opus_96()).await.unwrap();
    let (first, second) = tokio::join!(collect(first), collect(second));

    assert_eq!(first.unwrap(), second.unwrap());
    assert_eq!(
        std::fs::read_to_string(state.path().join("starts"))
            .unwrap()
            .lines()
            .count(),
        1,
        "同一缓存键只能由一个 FFmpeg 构建"
    );
}

struct BlockingPutStore {
    inner: MemoryStore,
    put_started: tokio::sync::Notify,
    release_put: tokio::sync::Notify,
    put_cancelled: Arc<AtomicBool>,
}

impl BlockingPutStore {
    fn new() -> Self {
        Self {
            inner: MemoryStore::new(),
            put_started: tokio::sync::Notify::new(),
            release_put: tokio::sync::Notify::new(),
            put_cancelled: Arc::new(AtomicBool::new(false)),
        }
    }
}

struct PutCancellationMark {
    cancelled: Arc<AtomicBool>,
    completed: bool,
}

impl Drop for PutCancellationMark {
    fn drop(&mut self) {
        if !self.completed {
            self.cancelled.store(true, Ordering::SeqCst);
        }
    }
}

#[async_trait]
impl ObjectStore for BlockingPutStore {
    async fn list(&self, prefix: &str, token: Option<String>) -> StoreResult<ListPage> {
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
        let mut mark = PutCancellationMark {
            cancelled: self.put_cancelled.clone(),
            completed: false,
        };
        self.put_started.notify_one();
        self.release_put.notified().await;
        let result = self.inner.put_file(key, path).await;
        mark.completed = true;
        result
    }

    async fn delete(&self, key: &str) -> StoreResult<()> {
        self.inner.delete(key).await
    }

    async fn head(&self, key: &str) -> StoreResult<ObjectMeta> {
        self.inner.head(key).await
    }
}

#[cfg(unix)]
#[tokio::test]
async fn consumer_drop_during_cache_upload_cancels_put_file() {
    use std::os::unix::fs::PermissionsExt;

    let (index, _db_dir) = temp_index().await;
    let state = tempfile::tempdir().unwrap();
    let script = state.path().join("cache-upload-ffmpeg.sh");
    std::fs::write(&script, "#!/bin/sh\nprintf cached\n").unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let store = Arc::new(BlockingPutStore::new());
    let track = insert_track(
        &index,
        &store.inner,
        "library/upload.flac",
        fixture("a.flac"),
    )
    .await;
    let id = track.id;
    let transcoder = Transcoder::with_concurrency(store.clone(), index.clone(), script, 1);
    let stream = transcoder.stream(track, opus_96()).await.unwrap();
    let consumer = tokio::spawn(collect(stream));
    tokio::time::timeout(Duration::from_secs(2), store.put_started.notified())
        .await
        .expect("应进入缓存 put_file");

    consumer.abort();
    let cancelled = tokio::time::timeout(Duration::from_millis(300), async {
        while !store.put_cancelled.load(Ordering::SeqCst) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .is_ok();
    store.release_put.notify_waiters();
    tokio::time::sleep(Duration::from_millis(50)).await;

    assert!(cancelled, "调用方断开必须取消正在等待的 put_file");
    assert!(index
        .transcode_cache()
        .get(id, "opus", 96)
        .await
        .unwrap()
        .is_none());
    assert!(store
        .list(&format!("transcode/{id}/"), None)
        .await
        .unwrap()
        .entries
        .is_empty());
}

fn assert_playable(path: &Path) {
    let status = std::process::Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_name",
            "-of",
            "default=nw=1",
        ])
        .arg(path)
        .status()
        .expect("ffprobe 应已随 FFmpeg 安装");
    assert!(status.success(), "转码输出应可被 ffprobe 解析");
}

async fn wait_for_cleanup(index: &Index, store: &MemoryStore, track_id: i64) {
    for _ in 0..100 {
        let row_absent = index
            .transcode_cache()
            .get(track_id, "opus", 96)
            .await
            .unwrap()
            .is_none();
        let object_absent = store
            .list(&format!("transcode/{track_id}/"), None)
            .await
            .unwrap()
            .entries
            .is_empty();
        if row_absent && object_absent {
            tokio::time::sleep(Duration::from_millis(50)).await;
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[cfg(unix)]
fn process_alive(pid: u32) -> bool {
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}
