//! 曲库访问控制强制（设计文档 §6）。
//!
//! 覆盖查询时可见性判定的核心不变量：默认开放、最具体作用域优先、管理员绕过、
//! 查询时评估（新入库曲目自动继承专辑/艺人规则）。用临时 SQLite 文件做集成测试。

use contract::{Principal, PrincipalType, ScopeType};
use music_server::index::{Index, NewTrack};

/// 在临时目录创建并连接一个全新索引；返回 TempDir 保活。
async fn temp_index() -> (Index, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("music.sqlite");
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
