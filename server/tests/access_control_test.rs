//! 曲库访问控制强制（设计文档 §6）。
//!
//! 覆盖查询时可见性判定的核心不变量：默认开放、最具体作用域优先、管理员绕过、
//! 查询时评估（新入库曲目自动继承专辑/艺人规则）。用临时 SQLite 文件做集成测试。

use contract::{Principal, PrincipalType, ScopeType};
use yevune_server::index::{Index, NewTrack};

/// 在临时目录创建并连接一个全新索引；返回 TempDir 保活。
async fn temp_index() -> (Index, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("yevune.sqlite");
    let index = Index::connect(&path).await.expect("连接并迁移失败");
    (index, dir)
}

/// 构造一条最小 NewTrack（可指定专辑/艺人/流派外键）。
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

/// 建一个用户并（可选）赋予角色，返回用户 id。
async fn user_with_roles(index: &Index, name: &str, roles: &[&str]) -> i64 {
    let uid = index.users().create_user(name, "enc").await.unwrap();
    for r in roles {
        let rid = match index.roles().get_role_by_name(r).await.unwrap() {
            Some(role) => role.id.parse().unwrap(),
            None => index.roles().create_role(r, false).await.unwrap(),
        };
        index.roles().assign(uid, rid).await.unwrap();
    }
    uid
}

/// 取（必要时建）一个角色 id。
async fn role_id(index: &Index, name: &str) -> i64 {
    match index.roles().get_role_by_name(name).await.unwrap() {
        Some(role) => role.id.parse().unwrap(),
        None => index.roles().create_role(name, false).await.unwrap(),
    }
}

/// 允许名单里放一个用户主体。
fn grant_user(uid: i64) -> Principal {
    Principal {
        principal_type: PrincipalType::User,
        id: uid.to_string(),
    }
}

/// 允许名单里放一个角色主体。
fn grant_role(rid: i64) -> Principal {
    Principal {
        principal_type: PrincipalType::Role,
        id: rid.to_string(),
    }
}

#[tokio::test]
async fn 受限曲目仅授权用户与管理员可见() {
    let (index, _dir) = temp_index().await;
    let alice = user_with_roles(&index, "alice", &[]).await; // 授权
    let bob = user_with_roles(&index, "bob", &[]).await; // 未授权
    let root = user_with_roles(&index, "root", &["admin"]).await; // 管理员
    let tid = index
        .media()
        .upsert_track(&new_track("受限曲", None, None, "k1"))
        .await
        .unwrap();
    // 曲目级规则：仅 alice 可见。
    index
        .access()
        .set_rule(
            ScopeType::Track,
            &tid.to_string(),
            None,
            &[grant_user(alice)],
        )
        .await
        .unwrap();

    let ac = index.access_control();
    let v_alice = ac.resolve_viewer(alice).await.unwrap();
    let v_bob = ac.resolve_viewer(bob).await.unwrap();
    let v_root = ac.resolve_viewer(root).await.unwrap();

    assert!(
        ac.can_access_track(&v_alice, tid).await.unwrap(),
        "授权用户可见"
    );
    assert!(
        !ac.can_access_track(&v_bob, tid).await.unwrap(),
        "未授权用户不可见"
    );
    assert!(
        ac.can_access_track(&v_root, tid).await.unwrap(),
        "管理员绕过全可见"
    );
}

#[tokio::test]
async fn 角色授权对角色内外用户区分生效() {
    let (index, _dir) = temp_index().await;
    let kid_role = role_id(&index, "孩子").await;
    let child = user_with_roles(&index, "child", &["孩子"]).await;
    let outsider = user_with_roles(&index, "outsider", &[]).await;
    let tid = index
        .media()
        .upsert_track(&new_track("儿歌", None, None, "k1"))
        .await
        .unwrap();
    index
        .access()
        .set_rule(
            ScopeType::Track,
            &tid.to_string(),
            None,
            &[grant_role(kid_role)],
        )
        .await
        .unwrap();

    let ac = index.access_control();
    let v_child = ac.resolve_viewer(child).await.unwrap();
    let v_out = ac.resolve_viewer(outsider).await.unwrap();
    assert!(
        ac.can_access_track(&v_child, tid).await.unwrap(),
        "角色内用户可见"
    );
    assert!(
        !ac.can_access_track(&v_out, tid).await.unwrap(),
        "角色外用户不可见"
    );
}

#[tokio::test]
async fn 曲目级规则覆盖专辑级() {
    let (index, _dir) = temp_index().await;
    let alice = user_with_roles(&index, "alice", &[]).await;
    let bob = user_with_roles(&index, "bob", &[]).await;
    let album = index
        .media()
        .upsert_album("专辑A", None, None, None)
        .await
        .unwrap();
    // 专辑级仅 alice；曲目级改为仅 bob（更具体，应覆盖专辑级）。
    let tid = index
        .media()
        .upsert_track(&new_track("曲1", Some(album), None, "k1"))
        .await
        .unwrap();
    index
        .access()
        .set_rule(
            ScopeType::Album,
            &album.to_string(),
            None,
            &[grant_user(alice)],
        )
        .await
        .unwrap();
    index
        .access()
        .set_rule(ScopeType::Track, &tid.to_string(), None, &[grant_user(bob)])
        .await
        .unwrap();

    let ac = index.access_control();
    let v_alice = ac.resolve_viewer(alice).await.unwrap();
    let v_bob = ac.resolve_viewer(bob).await.unwrap();
    assert!(
        !ac.can_access_track(&v_alice, tid).await.unwrap(),
        "曲目级(仅 bob)覆盖专辑级(仅 alice)：alice 不可见"
    );
    assert!(
        ac.can_access_track(&v_bob, tid).await.unwrap(),
        "曲目级(仅 bob)覆盖专辑级：bob 可见"
    );
}

#[tokio::test]
async fn 专辑级规则被新入库曲目查询时继承() {
    let (index, _dir) = temp_index().await;
    let alice = user_with_roles(&index, "alice", &[]).await;
    let bob = user_with_roles(&index, "bob", &[]).await;
    let album = index
        .media()
        .upsert_album("受限专辑", None, None, None)
        .await
        .unwrap();
    // 先给专辑设规则（仅 alice），之后再"扫入"新曲目。
    index
        .access()
        .set_rule(
            ScopeType::Album,
            &album.to_string(),
            None,
            &[grant_user(alice)],
        )
        .await
        .unwrap();
    let tid = index
        .media()
        .upsert_track(&new_track("后入库曲", Some(album), None, "k1"))
        .await
        .unwrap();

    let ac = index.access_control();
    let v_alice = ac.resolve_viewer(alice).await.unwrap();
    let v_bob = ac.resolve_viewer(bob).await.unwrap();
    assert!(
        ac.can_access_track(&v_alice, tid).await.unwrap(),
        "继承专辑规则：授权者可见"
    );
    assert!(
        !ac.can_access_track(&v_bob, tid).await.unwrap(),
        "查询时评估：新入库曲目自动继承专辑规则，未授权者不可见"
    );
}

#[tokio::test]
async fn 艺人级规则覆盖整批曲目而更具体专辑可放开() {
    let (index, _dir) = temp_index().await;
    let alice = user_with_roles(&index, "alice", &[]).await;
    let bob = user_with_roles(&index, "bob", &[]).await;
    let artist = index.media().upsert_artist("受限艺人").await.unwrap();
    let open_album = index
        .media()
        .upsert_album("放开专辑", Some(artist), None, None)
        .await
        .unwrap();
    // 艺人级仅 alice；其下某专辑单独放开给 bob（更具体）。
    let restricted = index
        .media()
        .upsert_track(&new_track("受限曲", None, Some(artist), "k1"))
        .await
        .unwrap();
    let opened = index
        .media()
        .upsert_track(&new_track("放开曲", Some(open_album), Some(artist), "k2"))
        .await
        .unwrap();
    index
        .access()
        .set_rule(
            ScopeType::Artist,
            &artist.to_string(),
            None,
            &[grant_user(alice)],
        )
        .await
        .unwrap();
    index
        .access()
        .set_rule(
            ScopeType::Album,
            &open_album.to_string(),
            None,
            &[grant_user(bob)],
        )
        .await
        .unwrap();

    let ac = index.access_control();
    let v_bob = ac.resolve_viewer(bob).await.unwrap();
    assert!(
        !ac.can_access_track(&v_bob, restricted).await.unwrap(),
        "艺人级限制：bob 对无更具体规则的曲目不可见"
    );
    assert!(
        ac.can_access_track(&v_bob, opened).await.unwrap(),
        "更具体的专辑规则放开：bob 对该专辑曲目可见"
    );
}

#[tokio::test]
async fn 流派级规则生效() {
    let (index, _dir) = temp_index().await;
    let alice = user_with_roles(&index, "alice", &[]).await;
    let bob = user_with_roles(&index, "bob", &[]).await;
    let mut t = new_track("爵士曲", None, None, "k1");
    t.genre = Some("Jazz".into());
    let tid = index.media().upsert_track(&t).await.unwrap();
    index
        .access()
        .set_rule(ScopeType::Genre, "Jazz", None, &[grant_user(alice)])
        .await
        .unwrap();

    let ac = index.access_control();
    let v_alice = ac.resolve_viewer(alice).await.unwrap();
    let v_bob = ac.resolve_viewer(bob).await.unwrap();
    assert!(
        ac.can_access_track(&v_alice, tid).await.unwrap(),
        "流派授权者可见"
    );
    assert!(
        !ac.can_access_track(&v_bob, tid).await.unwrap(),
        "流派未授权者不可见"
    );
}

#[tokio::test]
async fn 流派覆盖后按有效流派强制规则() {
    let (index, _dir) = temp_index().await;
    let bob = user_with_roles(&index, "bob", &["kids"]).await;
    let root = user_with_roles(&index, "root", &["admin"]).await;
    let kids_role = role_id(&index, "kids").await;
    let mut track = new_track("覆盖流派曲目", None, None, "override-genre.flac");
    track.genre = Some("Rock".into());
    let track_id = index.media().upsert_track(&track).await.unwrap();
    index
        .media()
        .set_tag_overrides(track_id, &[("genre", "Kids")])
        .await
        .unwrap();
    index
        .access()
        .set_rule(ScopeType::Genre, "Kids", None, &[])
        .await
        .unwrap();

    let access = index.access_control();
    let bob_viewer = access.resolve_viewer(bob).await.unwrap();
    let root_viewer = access.resolve_viewer(root).await.unwrap();
    assert!(
        !access
            .can_access_track(&bob_viewer, track_id)
            .await
            .unwrap(),
        "空允许名单必须按覆盖后的 Kids 流派拒绝普通用户"
    );
    assert!(
        access
            .can_access_track(&root_viewer, track_id)
            .await
            .unwrap(),
        "管理员始终绕过流派规则"
    );

    index
        .access()
        .set_rule(ScopeType::Genre, "Kids", None, &[grant_role(kids_role)])
        .await
        .unwrap();
    assert!(
        access
            .can_access_track(&bob_viewer, track_id)
            .await
            .unwrap(),
        "角色授权应让普通用户恢复可见"
    );
}

#[tokio::test]
async fn 无规则时曲目对所有用户可见() {
    let (index, _dir) = temp_index().await;
    let uid = user_with_roles(&index, "alice", &[]).await;
    let tid = index
        .media()
        .upsert_track(&new_track("开放曲", None, None, "k1"))
        .await
        .unwrap();

    let ac = index.access_control();
    let viewer = ac.resolve_viewer(uid).await.unwrap();
    assert!(
        ac.can_access_track(&viewer, tid).await.unwrap(),
        "默认开放：无任何规则时应可见"
    );
}

// ───────────── 带可见性过滤的读方法（供各曲库读路径复用）─────────────

#[tokio::test]
async fn 过滤_get_track_visible_对未授权隐藏对管理员可见() {
    let (index, _dir) = temp_index().await;
    let alice = user_with_roles(&index, "alice", &[]).await;
    let bob = user_with_roles(&index, "bob", &[]).await;
    let root = user_with_roles(&index, "root", &["admin"]).await;
    let tid = index
        .media()
        .upsert_track(&new_track("受限曲", None, None, "k1"))
        .await
        .unwrap();
    index
        .access()
        .set_rule(
            ScopeType::Track,
            &tid.to_string(),
            None,
            &[grant_user(alice)],
        )
        .await
        .unwrap();

    let ac = index.access_control();
    let (v_alice, v_bob, v_root) = (
        ac.resolve_viewer(alice).await.unwrap(),
        ac.resolve_viewer(bob).await.unwrap(),
        ac.resolve_viewer(root).await.unwrap(),
    );
    assert!(index
        .media()
        .get_track_visible(&v_bob, tid)
        .await
        .unwrap()
        .is_none());
    assert!(index
        .media()
        .get_track_visible(&v_alice, tid)
        .await
        .unwrap()
        .is_some());
    assert!(index
        .media()
        .get_track_visible(&v_root, tid)
        .await
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn 过滤_专辑曲目列表仅含可见曲目() {
    let (index, _dir) = temp_index().await;
    let alice = user_with_roles(&index, "alice", &[]).await;
    let bob = user_with_roles(&index, "bob", &[]).await;
    let album = index
        .media()
        .upsert_album("混合专辑", None, None, None)
        .await
        .unwrap();
    let open = index
        .media()
        .upsert_track(&new_track("开放曲", Some(album), None, "k1"))
        .await
        .unwrap();
    let secret = index
        .media()
        .upsert_track(&new_track("隐藏曲", Some(album), None, "k2"))
        .await
        .unwrap();
    index
        .access()
        .set_rule(
            ScopeType::Track,
            &secret.to_string(),
            None,
            &[grant_user(alice)],
        )
        .await
        .unwrap();

    let ac = index.access_control();
    let v_bob = ac.resolve_viewer(bob).await.unwrap();
    let v_alice = ac.resolve_viewer(alice).await.unwrap();

    let bob_tracks = index
        .media()
        .album_tracks_visible(&v_bob, album)
        .await
        .unwrap();
    let bob_ids: Vec<String> = bob_tracks.iter().map(|t| t.id.clone()).collect();
    assert_eq!(bob_ids, vec![open.to_string()], "bob 只看到开放曲");

    let alice_tracks = index
        .media()
        .album_tracks_visible(&v_alice, album)
        .await
        .unwrap();
    assert_eq!(alice_tracks.len(), 2, "alice 看到全部");
    let _ = secret;
}

#[tokio::test]
async fn 过滤_专辑列表隐藏无可见曲目的专辑() {
    let (index, _dir) = temp_index().await;
    let alice = user_with_roles(&index, "alice", &[]).await;
    let bob = user_with_roles(&index, "bob", &[]).await;
    let restricted_album = index
        .media()
        .upsert_album("受限专辑", None, None, None)
        .await
        .unwrap();
    let open_album = index
        .media()
        .upsert_album("开放专辑", None, None, None)
        .await
        .unwrap();
    index
        .media()
        .upsert_track(&new_track("受限曲", Some(restricted_album), None, "k1"))
        .await
        .unwrap();
    index
        .media()
        .upsert_track(&new_track("开放曲", Some(open_album), None, "k2"))
        .await
        .unwrap();
    index
        .access()
        .set_rule(
            ScopeType::Album,
            &restricted_album.to_string(),
            None,
            &[grant_user(alice)],
        )
        .await
        .unwrap();

    let ac = index.access_control();
    let v_bob = ac.resolve_viewer(bob).await.unwrap();
    let v_alice = ac.resolve_viewer(alice).await.unwrap();

    let bob_albums = index.media().list_albums_visible(&v_bob).await.unwrap();
    let bob_ids: Vec<String> = bob_albums.iter().map(|a| a.id.clone()).collect();
    assert!(bob_ids.contains(&open_album.to_string()), "bob 见开放专辑");
    assert!(
        !bob_ids.contains(&restricted_album.to_string()),
        "bob 不见无可见曲目的受限专辑"
    );
    assert!(index
        .media()
        .get_album_visible(&v_bob, restricted_album)
        .await
        .unwrap()
        .is_none());

    let alice_albums = index.media().list_albums_visible(&v_alice).await.unwrap();
    assert_eq!(alice_albums.len(), 2, "alice 见全部专辑");
}

#[tokio::test]
async fn 过滤_艺人及其专辑按可见性收敛() {
    let (index, _dir) = temp_index().await;
    let alice = user_with_roles(&index, "alice", &[]).await;
    let bob = user_with_roles(&index, "bob", &[]).await;
    let artist = index.media().upsert_artist("神秘艺人").await.unwrap();
    let restricted_album = index
        .media()
        .upsert_album("受限专辑", Some(artist), None, None)
        .await
        .unwrap();
    index
        .media()
        .upsert_track(&new_track(
            "受限曲",
            Some(restricted_album),
            Some(artist),
            "k1",
        ))
        .await
        .unwrap();
    index
        .access()
        .set_rule(
            ScopeType::Album,
            &restricted_album.to_string(),
            None,
            &[grant_user(alice)],
        )
        .await
        .unwrap();

    let ac = index.access_control();
    let v_bob = ac.resolve_viewer(bob).await.unwrap();

    // 该艺人当前对 bob 无任何可见曲目 → 不出现在列表。
    let artists = index.media().list_artists_visible(&v_bob).await.unwrap();
    assert!(
        !artists.iter().any(|a| a.id == artist.to_string()),
        "无可见曲目的艺人不出现"
    );
    assert!(index
        .media()
        .get_artist_visible(&v_bob, artist)
        .await
        .unwrap()
        .is_none());

    // 在该艺人下新增一张开放专辑与曲目 → bob 现在能看到艺人，且只看到开放专辑。
    let open_album = index
        .media()
        .upsert_album("开放专辑", Some(artist), None, None)
        .await
        .unwrap();
    index
        .media()
        .upsert_track(&new_track("开放曲", Some(open_album), Some(artist), "k2"))
        .await
        .unwrap();

    let artists = index.media().list_artists_visible(&v_bob).await.unwrap();
    assert!(
        artists.iter().any(|a| a.id == artist.to_string()),
        "有可见曲目后艺人出现"
    );
    let albums = index
        .media()
        .artist_albums_visible(&v_bob, artist)
        .await
        .unwrap();
    let ids: Vec<String> = albums.iter().map(|a| a.id.clone()).collect();
    assert_eq!(ids, vec![open_album.to_string()], "只列出有可见曲目的专辑");
}

#[tokio::test]
async fn 过滤_search_visible_过滤受限命中() {
    let (index, _dir) = temp_index().await;
    let alice = user_with_roles(&index, "alice", &[]).await;
    let bob = user_with_roles(&index, "bob", &[]).await;
    let tid = index
        .media()
        .upsert_track(&new_track("绝密档案", None, None, "k1"))
        .await
        .unwrap();
    index
        .access()
        .set_rule(
            ScopeType::Track,
            &tid.to_string(),
            None,
            &[grant_user(alice)],
        )
        .await
        .unwrap();

    let ac = index.access_control();
    let v_bob = ac.resolve_viewer(bob).await.unwrap();
    let v_alice = ac.resolve_viewer(alice).await.unwrap();

    let bob_hits = index
        .media()
        .search_visible(&v_bob, "绝密档", 20)
        .await
        .unwrap();
    assert!(
        bob_hits.tracks.is_empty(),
        "受限曲目不出现在未授权者搜索结果"
    );
    let alice_hits = index
        .media()
        .search_visible(&v_alice, "绝密档", 20)
        .await
        .unwrap();
    assert_eq!(alice_hits.tracks.len(), 1, "授权者可搜到");
}
