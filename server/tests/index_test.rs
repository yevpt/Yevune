//! index 层集成测试：迁移、模式、WAL 与各仓储行为（临时 SQLite 文件）。

use yevune_server::index::{Index, NewTrack, NewTranscodeCache};

/// 在临时目录创建并连接一个全新索引；返回 TempDir 保活。
async fn temp_index() -> (Index, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("yevune.sqlite");
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

#[tokio::test]
async fn transcode_cache_仓储可登记查询更新与删除() {
    let (index, _dir) = temp_index().await;
    let track_id = index
        .media()
        .upsert_track(&new_track("Cache Me", None, None, "library/cache.flac"))
        .await
        .unwrap();
    let cache = index.transcode_cache();

    cache
        .upsert(&NewTranscodeCache {
            track_id,
            format: "opus".into(),
            bitrate: 128,
            object_key: format!("transcode/{track_id}/opus_128.opus"),
            size: 1234,
        })
        .await
        .unwrap();
    let found = cache.get(track_id, "opus", 128).await.unwrap().unwrap();
    assert_eq!(
        found.object_key,
        format!("transcode/{track_id}/opus_128.opus")
    );
    assert_eq!(found.size, 1234);

    cache
        .upsert(&NewTranscodeCache {
            track_id,
            format: "opus".into(),
            bitrate: 128,
            object_key: format!("transcode/{track_id}/opus_128.opus"),
            size: 5678,
        })
        .await
        .unwrap();
    assert_eq!(
        cache
            .get(track_id, "opus", 128)
            .await
            .unwrap()
            .unwrap()
            .size,
        5678
    );

    cache.remove(track_id, "opus", 128).await.unwrap();
    assert!(cache.get(track_id, "opus", 128).await.unwrap().is_none());
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
            "library/jay/qingtian.flac",
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
    assert_eq!(track.path.as_deref(), Some("library/jay/qingtian.flac"));
}

#[tokio::test]
async fn media_upsert_track_幂等更新() {
    let (index, _dir) = temp_index().await;
    let media = index.media();
    let id1 = media
        .upsert_track(&new_track("旧标题", None, None, "library/x.flac"))
        .await
        .unwrap();
    let id2 = media
        .upsert_track(&new_track("新标题", None, None, "library/x.flac"))
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
            "library/a.flac",
        ))
        .await
        .unwrap();
    media
        .upsert_track(&new_track(
            "以父之名",
            Some(album),
            Some(artist),
            "library/b.flac",
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
        .upsert_track(&new_track("晴天", Some(al), Some(ar), "library/a.flac"))
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
        .upsert_track(&new_track("待删", None, None, "library/del.flac"))
        .await
        .unwrap();
    assert!(media
        .delete_by_object_key("library/del.flac")
        .await
        .unwrap());
    assert!(media.get_track(id).await.unwrap().is_none());
    assert!(!media
        .delete_by_object_key("library/del.flac")
        .await
        .unwrap());
}

// ─────────────────────────── User / Role ───────────────────────────

#[tokio::test]
async fn user_创建读取与改密() {
    let (index, _dir) = temp_index().await;
    let users = index.users();
    let id = users.create_user("papa", "enc-1").await.unwrap();

    let u = users.get_user(id).await.unwrap().expect("应存在");
    assert_eq!(u.id, id.to_string());
    assert_eq!(u.name, "papa");
    assert!(!u.admin);
    assert!(u.roles.is_empty());

    // 密码不进入 DTO，但可供认证层取回
    assert_eq!(
        users.password_enc("papa").await.unwrap().as_deref(),
        Some("enc-1")
    );
    assert!(users.change_password(id, "enc-2").await.unwrap());
    assert_eq!(
        users.password_enc("papa").await.unwrap().as_deref(),
        Some("enc-2")
    );

    let by_name = users.get_user_by_name("papa").await.unwrap().unwrap();
    assert_eq!(by_name.id, id.to_string());
}

#[tokio::test]
async fn user_删除() {
    let (index, _dir) = temp_index().await;
    let users = index.users();
    let id = users.create_user("kid", "e").await.unwrap();
    assert!(users.delete_user(id).await.unwrap());
    assert!(users.get_user(id).await.unwrap().is_none());
    assert!(!users.delete_user(id).await.unwrap());
}

#[tokio::test]
async fn role_分配解除与_admin_判定() {
    let (index, _dir) = temp_index().await;
    let users = index.users();
    let roles = index.roles();

    let uid = users.create_user("papa", "e").await.unwrap();
    let admin = roles.create_role("admin", true).await.unwrap();
    let member = roles.create_role("member", true).await.unwrap();

    assert!(!roles.is_admin(uid).await.unwrap());
    roles.assign(uid, admin).await.unwrap();
    roles.assign(uid, member).await.unwrap();
    // 幂等：重复分配不报错
    roles.assign(uid, admin).await.unwrap();

    assert!(roles.is_admin(uid).await.unwrap());
    let list = roles.roles_of(uid).await.unwrap();
    assert_eq!(list.len(), 2);

    // DTO 反映角色与 admin
    let u = users.get_user(uid).await.unwrap().unwrap();
    assert!(u.admin);
    assert!(u.roles.iter().any(|r| r == "admin"));

    assert!(roles.unassign(uid, admin).await.unwrap());
    assert!(!roles.is_admin(uid).await.unwrap());
    assert!(!roles.unassign(uid, admin).await.unwrap());
}

#[tokio::test]
async fn role_列举与删除() {
    let (index, _dir) = temp_index().await;
    let roles = index.roles();
    let r = roles.create_role("孩子", false).await.unwrap();
    assert!(roles.get_role_by_name("孩子").await.unwrap().is_some());
    assert_eq!(roles.list_roles().await.unwrap().len(), 1);
    assert!(roles.delete_role(r).await.unwrap());
    assert!(roles.get_role_by_name("孩子").await.unwrap().is_none());
}

// ─────────────────────────── Playlist / Folder ───────────────────────────

async fn seed_user(index: &Index, name: &str) -> i64 {
    index.users().create_user(name, "e").await.unwrap()
}

#[tokio::test]
async fn playlist_文件夹树_嵌套与_owner_隔离() {
    let (index, _dir) = temp_index().await;
    let pl = index.playlists();
    let papa = seed_user(&index, "papa").await;
    let kid = seed_user(&index, "kid").await;

    // papa: 中文 → 华语经典(子)
    let zh = pl.create_folder(papa, "中文", None).await.unwrap();
    let _classic = pl.create_folder(papa, "华语经典", Some(zh)).await.unwrap();
    // kid 的树
    pl.create_folder(kid, "儿歌", None).await.unwrap();

    let papa_folders = pl.list_folders(papa).await.unwrap();
    assert_eq!(papa_folders.len(), 2, "papa 只应见到自己的两个文件夹");
    let child = papa_folders
        .iter()
        .find(|f| f.name == "华语经典")
        .expect("应有子文件夹");
    assert_eq!(child.parent_id.as_deref(), Some(zh.to_string().as_str()));

    // owner 隔离
    assert!(!papa_folders.iter().any(|f| f.name == "儿歌"));
    assert_eq!(pl.list_folders(kid).await.unwrap().len(), 1);
}

#[tokio::test]
async fn playlist_文件夹重命名与移动() {
    let (index, _dir) = temp_index().await;
    let pl = index.playlists();
    let papa = seed_user(&index, "papa").await;
    let a = pl.create_folder(papa, "A", None).await.unwrap();
    let b = pl.create_folder(papa, "B", None).await.unwrap();

    assert!(pl.rename_folder(a, "A-改").await.unwrap());
    assert!(pl.move_folder(b, Some(a)).await.unwrap());

    let folders = pl.list_folders(papa).await.unwrap();
    let moved = folders.iter().find(|f| f.id == b.to_string()).unwrap();
    assert_eq!(moved.parent_id.as_deref(), Some(a.to_string().as_str()));
    assert!(folders.iter().any(|f| f.name == "A-改"));
}

#[tokio::test]
async fn playlist_删除文件夹级联子文件夹() {
    let (index, _dir) = temp_index().await;
    let pl = index.playlists();
    let papa = seed_user(&index, "papa").await;
    let parent = pl.create_folder(papa, "父", None).await.unwrap();
    pl.create_folder(papa, "子", Some(parent)).await.unwrap();

    assert!(pl.delete_folder(parent).await.unwrap());
    assert_eq!(
        pl.list_folders(papa).await.unwrap().len(),
        0,
        "子文件夹应被级联删除"
    );
}

#[tokio::test]
async fn playlist_crud_与移动() {
    let (index, _dir) = temp_index().await;
    let pl = index.playlists();
    let papa = seed_user(&index, "papa").await;
    let folder = pl.create_folder(papa, "中文", None).await.unwrap();

    let id = pl.create_playlist(papa, "精选", None).await.unwrap();
    let got = pl.get_playlist(id).await.unwrap().expect("应存在");
    assert_eq!(got.name, "精选");
    assert_eq!(got.owner_id, papa.to_string());
    assert_eq!(got.song_count, 0);
    assert!(got.folder_id.is_none());

    assert!(pl.update_playlist(id, "精选2", Some("备注")).await.unwrap());
    assert!(pl.move_playlist(id, Some(folder)).await.unwrap());
    let moved = pl.get_playlist(id).await.unwrap().unwrap();
    assert_eq!(moved.name, "精选2");
    assert_eq!(moved.comment.as_deref(), Some("备注"));
    assert_eq!(
        moved.folder_id.as_deref(),
        Some(folder.to_string().as_str())
    );

    assert!(pl.delete_playlist(id).await.unwrap());
    assert!(pl.get_playlist(id).await.unwrap().is_none());
}

#[tokio::test]
async fn playlist_曲目有序增删与聚合() {
    let (index, _dir) = temp_index().await;
    let pl = index.playlists();
    let media = index.media();
    let papa = seed_user(&index, "papa").await;

    let t1 = media
        .upsert_track(&new_track("A", None, None, "library/a.flac"))
        .await
        .unwrap();
    let t2 = media
        .upsert_track(&new_track("B", None, None, "library/b.flac"))
        .await
        .unwrap();
    let pid = pl.create_playlist(papa, "L", None).await.unwrap();

    pl.set_tracks(pid, &[t2, t1]).await.unwrap();
    assert_eq!(
        pl.track_ids(pid).await.unwrap(),
        vec![t2, t1],
        "应保持插入顺序"
    );

    let dto = pl.get_playlist(pid).await.unwrap().unwrap();
    assert_eq!(dto.song_count, 2);
    assert_eq!(dto.duration, 400);

    // 整体替换
    pl.set_tracks(pid, &[t1]).await.unwrap();
    assert_eq!(pl.track_ids(pid).await.unwrap(), vec![t1]);
}

#[tokio::test]
async fn playlist_列举按_owner() {
    let (index, _dir) = temp_index().await;
    let pl = index.playlists();
    let papa = seed_user(&index, "papa").await;
    let kid = seed_user(&index, "kid").await;
    pl.create_playlist(papa, "P1", None).await.unwrap();
    pl.create_playlist(papa, "P2", None).await.unwrap();
    pl.create_playlist(kid, "K1", None).await.unwrap();

    assert_eq!(pl.list_playlists(papa).await.unwrap().len(), 2);
    assert_eq!(pl.list_playlists(kid).await.unwrap().len(), 1);
}

// ─────────────────────────── Annotation ───────────────────────────

#[tokio::test]
async fn annotation_收藏评分与播放计数_按用户隔离() {
    let (index, _dir) = temp_index().await;
    let papa = seed_user(&index, "papa").await;
    let kid = seed_user(&index, "kid").await;
    let ann = index.annotations();

    // papa 收藏 + 评分 + 两次播放曲目 1
    ann.star(papa, "track", 1).await.unwrap();
    ann.set_rating(papa, "track", 1, Some(5)).await.unwrap();
    ann.scrobble(papa, "track", 1).await.unwrap();
    ann.scrobble(papa, "track", 1).await.unwrap();

    let a = ann.get(papa, "track", 1).await.unwrap().expect("应有标注");
    assert!(a.starred_at.is_some());
    assert_eq!(a.rating, Some(5));
    assert_eq!(a.play_count, 2);
    assert!(a.last_played.is_some());

    // kid 对同一曲目无标注（按用户隔离）
    assert!(ann.get(kid, "track", 1).await.unwrap().is_none());

    // 取消收藏保留计数
    ann.unstar(papa, "track", 1).await.unwrap();
    let a2 = ann.get(papa, "track", 1).await.unwrap().unwrap();
    assert!(a2.starred_at.is_none());
    assert_eq!(a2.play_count, 2);
}

// ─────────────────────────── Access control ───────────────────────────

use contract::{Principal, PrincipalType, ScopeType};
use yevune_server::index::TrackScope;

fn user_grant(id: i64) -> Principal {
    Principal {
        principal_type: PrincipalType::User,
        id: id.to_string(),
    }
}

#[tokio::test]
async fn access_规则_upsert_读取与删除() {
    let (index, _dir) = temp_index().await;
    let creator = seed_user(&index, "admin").await;
    let acc = index.access();

    let rid = acc
        .set_rule(
            ScopeType::Album,
            "10",
            Some(creator),
            &[user_grant(1), user_grant(2)],
        )
        .await
        .unwrap();
    assert!(rid > 0);

    let rule = acc
        .get_rule(ScopeType::Album, "10")
        .await
        .unwrap()
        .expect("应存在");
    assert_eq!(rule.scope_type, ScopeType::Album);
    assert_eq!(rule.scope_id, "10");
    assert_eq!(rule.grants.len(), 2);

    // upsert 覆盖名单
    acc.set_rule(ScopeType::Album, "10", Some(1), &[user_grant(3)])
        .await
        .unwrap();
    let rule2 = acc.get_rule(ScopeType::Album, "10").await.unwrap().unwrap();
    assert_eq!(rule2.grants.len(), 1);
    assert_eq!(rule2.grants[0].id, "3");

    assert!(acc.delete_rule(ScopeType::Album, "10").await.unwrap());
    assert!(acc
        .get_rule(ScopeType::Album, "10")
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn access_最具体作用域优先() {
    let (index, _dir) = temp_index().await;
    let acc = index.access();

    // 艺人级限制给用户 1；专辑级限制给用户 2；曲目级限制给用户 3
    acc.set_rule(ScopeType::Artist, "100", None, &[user_grant(1)])
        .await
        .unwrap();
    acc.set_rule(ScopeType::Album, "10", None, &[user_grant(2)])
        .await
        .unwrap();
    acc.set_rule(ScopeType::Track, "1", None, &[user_grant(3)])
        .await
        .unwrap();

    let scope = TrackScope {
        track_id: 1,
        album_id: Some(10),
        artist_id: Some(100),
        genre: Some("Rock"),
    };
    let rule = acc
        .effective_rule(&scope)
        .await
        .unwrap()
        .expect("应命中规则");
    // 曲目级最具体
    assert_eq!(rule.scope_type, ScopeType::Track);
    assert_eq!(rule.grants[0].id, "3");

    // 无曲目规则时回落到专辑级
    acc.delete_rule(ScopeType::Track, "1").await.unwrap();
    let rule2 = acc.effective_rule(&scope).await.unwrap().unwrap();
    assert_eq!(rule2.scope_type, ScopeType::Album);
}

#[tokio::test]
async fn access_无规则则开放() {
    let (index, _dir) = temp_index().await;
    let acc = index.access();
    let scope = TrackScope {
        track_id: 1,
        album_id: Some(10),
        artist_id: Some(100),
        genre: Some("Rock"),
    };
    assert!(acc.effective_rule(&scope).await.unwrap().is_none());
}
