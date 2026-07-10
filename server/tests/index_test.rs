//! index 层集成测试：迁移、模式、WAL 与各仓储行为（临时 SQLite 文件）。

use music_server::index::{Index, NewTrack};

/// 在临时目录创建并连接一个全新索引；返回 TempDir 保活。
async fn temp_index() -> (Index, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("music.sqlite");
    let index = Index::connect(&path).await.expect("连接并迁移失败");
    (index, dir)
}

#[tokio::test]
async fn 迁移建立全部表() {
    let (index, _dir) = temp_index().await;
    let tables: Vec<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type = 'table'")
            .fetch_all(index.pool())
            .await
            .unwrap();

    for expected in [
        "users",
        "roles",
        "user_roles",
        "artists",
        "albums",
        "tracks",
        "annotations",
        "tag_overrides",
        "playlist_folders",
        "playlists",
        "playlist_tracks",
        "access_rules",
        "access_rule_grants",
        "transcode_cache",
        "scan_state",
    ] {
        assert!(
            tables.iter().any(|t| t == expected),
            "缺少表 `{expected}`，实际：{tables:?}"
        );
    }
}

#[tokio::test]
async fn wal_模式生效() {
    let (index, _dir) = temp_index().await;
    let mode: String = sqlx::query_scalar("PRAGMA journal_mode")
        .fetch_one(index.pool())
        .await
        .unwrap();
    assert_eq!(mode.to_lowercase(), "wal");
}

// ─────────────────────────── MediaRepo ───────────────────────────

/// 构造一条最小 NewTrack。
fn new_track(
    title: &str,
    album_id: Option<i64>,
    artist_id: Option<i64>,
    object_key: &str,
) -> NewTrack {
    NewTrack {
        title: title.into(),
        album_id,
        artist_id,
        track_no: Some(1),
        duration: Some(200),
        codec: Some("flac".into()),
        bitrate: Some(1000),
        size: Some(30_000_000),
        object_key: object_key.into(),
        ..Default::default()
    }
}

#[tokio::test]
async fn media_upsert_艺人去重() {
    let (index, _dir) = temp_index().await;
    let media = index.media();
    let a1 = media.upsert_artist("周杰伦").await.unwrap();
    let a2 = media.upsert_artist("周杰伦").await.unwrap();
    assert_eq!(a1, a2, "同名艺人应复用同一主键");
    let other = media.upsert_artist("林俊杰").await.unwrap();
    assert_ne!(a1, other);
}

#[tokio::test]
async fn media_upsert_track_并读取_dto() {
    let (index, _dir) = temp_index().await;
    let media = index.media();
    let artist = media.upsert_artist("周杰伦").await.unwrap();
    let album = media
        .upsert_album("叶惠美", Some(artist), Some(2003), Some("Mandopop"))
        .await
        .unwrap();
    let id = media
        .upsert_track(&new_track(
            "晴天",
            Some(album),
            Some(artist),
            "music/jay/qingtian.flac",
        ))
        .await
        .unwrap();

    let track = media.get_track(id).await.unwrap().expect("应存在");
    assert_eq!(track.id, id.to_string());
    assert_eq!(track.title, "晴天");
    assert_eq!(track.album.as_deref(), Some("叶惠美"));
    assert_eq!(track.album_id.as_deref(), Some(album.to_string().as_str()));
    assert_eq!(track.artist.as_deref(), Some("周杰伦"));
    assert_eq!(track.suffix.as_deref(), Some("flac"));
    assert_eq!(track.duration, 200);
}

#[tokio::test]
async fn media_upsert_track_幂等更新() {
    let (index, _dir) = temp_index().await;
    let media = index.media();
    let id1 = media
        .upsert_track(&new_track("旧标题", None, None, "music/x.flac"))
        .await
        .unwrap();
    let id2 = media
        .upsert_track(&new_track("新标题", None, None, "music/x.flac"))
        .await
        .unwrap();
    assert_eq!(id1, id2, "同 object_key 应更新而非新增");
    let track = media.get_track(id1).await.unwrap().unwrap();
    assert_eq!(track.title, "新标题");
}

#[tokio::test]
async fn media_get_album_聚合曲目数与时长() {
    let (index, _dir) = temp_index().await;
    let media = index.media();
    let artist = media.upsert_artist("周杰伦").await.unwrap();
    let album = media
        .upsert_album("叶惠美", Some(artist), Some(2003), Some("Mandopop"))
        .await
        .unwrap();
    media
        .upsert_track(&new_track(
            "晴天",
            Some(album),
            Some(artist),
            "music/a.flac",
        ))
        .await
        .unwrap();
    media
        .upsert_track(&new_track(
            "以父之名",
            Some(album),
            Some(artist),
            "music/b.flac",
        ))
        .await
        .unwrap();

    let dto = media.get_album(album).await.unwrap().expect("应存在");
    assert_eq!(dto.name, "叶惠美");
    assert_eq!(dto.artist.as_deref(), Some("周杰伦"));
    assert_eq!(dto.song_count, 2);
    assert_eq!(dto.duration, 400);
    assert_eq!(dto.year, Some(2003));
}

#[tokio::test]
async fn media_list_albums() {
    let (index, _dir) = temp_index().await;
    let media = index.media();
    let ar = media.upsert_artist("A").await.unwrap();
    media
        .upsert_album("专辑一", Some(ar), None, None)
        .await
        .unwrap();
    media
        .upsert_album("专辑二", Some(ar), None, None)
        .await
        .unwrap();
    let albums = media.list_albums().await.unwrap();
    assert_eq!(albums.len(), 2);
}

#[tokio::test]
async fn media_search_命中曲目与专辑() {
    let (index, _dir) = temp_index().await;
    let media = index.media();
    let ar = media.upsert_artist("周杰伦").await.unwrap();
    let al = media
        .upsert_album("叶惠美", Some(ar), None, None)
        .await
        .unwrap();
    media
        .upsert_track(&new_track("晴天", Some(al), Some(ar), "music/a.flac"))
        .await
        .unwrap();

    // trigram 支持中文子串（≥3 字符）
    let res = media.search("叶惠美", 10).await.unwrap();
    assert!(res.albums.iter().any(|a| a.name == "叶惠美"), "应搜到专辑");

    let res2 = media.search("周杰伦", 10).await.unwrap();
    assert!(
        res2.artists.iter().any(|a| a.name == "周杰伦"),
        "应搜到艺人"
    );
}

#[tokio::test]
async fn media_delete_by_object_key() {
    let (index, _dir) = temp_index().await;
    let media = index.media();
    let id = media
        .upsert_track(&new_track("待删", None, None, "music/del.flac"))
        .await
        .unwrap();
    assert!(media.delete_by_object_key("music/del.flac").await.unwrap());
    assert!(media.get_track(id).await.unwrap().is_none());
    assert!(!media.delete_by_object_key("music/del.flac").await.unwrap());
}
