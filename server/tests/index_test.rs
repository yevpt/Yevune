//! index 层集成测试：迁移、模式、WAL 与各仓储行为（临时 SQLite 文件）。

use music_server::index::Index;

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
