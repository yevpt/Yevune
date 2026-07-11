//! 端到端：OpenSubsonic 浏览/搜索读端点强制曲库访问控制（设计文档 §6 / 计划 T9）。
//!
//! 用真实 HTTP 请求驱动 [`music_server::app`]，断言受限内容对无授权用户
//! 在浏览/搜索/取详情各路径均不可见，对授权用户与管理员可见。

mod common;

use contract::{Principal, PrincipalType, ScopeType};
use serde_json::Value;

/// 内部主键 → OpenSubsonic 对外 opaque id（前缀区分实体类型）。
fn tr(id: i64) -> String {
    format!("tr-{id}")
}
fn al(id: i64) -> String {
    format!("al-{id}")
}
fn ar(id: i64) -> String {
    format!("ar-{id}")
}

/// 收集 getArtists/getIndexes 响应中的全部艺人 id。
fn artist_ids(resp: &Value, root: &str) -> Vec<String> {
    resp["subsonic-response"][root]["index"]
        .as_array()
        .map(|indexes| {
            indexes
                .iter()
                .flat_map(|idx| idx["artist"].as_array().cloned().unwrap_or_default())
                .filter_map(|a| a["id"].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// 收集数组元素的 id 字段。
fn ids(values: &Value) -> Vec<String> {
    values
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v["id"].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// 播种：两位普通用户 + 管理员 + 一张含公开/受限曲目的专辑 + 一位全受限艺人。
struct Fixture {
    alice: i64,
    bob: i64,
    root: i64,
    artist1: i64,
    artist2: i64,
    album1: i64,
    album2: i64,
    open_track: i64,
    secret_track: i64,
}

async fn seed(ctx: &common::Ctx) -> Fixture {
    let alice = ctx.create_user("alice", &[]).await;
    let bob = ctx.create_user("bob", &[]).await;
    let root = ctx.create_user("root", &["admin"]).await;
    let media = ctx.index.media();

    let artist1 = media.upsert_artist("歌手甲").await.unwrap();
    let album1 = media
        .upsert_album("专辑甲", Some(artist1), None, None)
        .await
        .unwrap();
    let open_track = media
        .upsert_track(&common::track(
            "公开曲目",
            Some(album1),
            Some(artist1),
            "a/open.flac",
        ))
        .await
        .unwrap();
    let secret_track = media
        .upsert_track(&common::track(
            "机密曲目",
            Some(album1),
            Some(artist1),
            "a/secret.flac",
        ))
        .await
        .unwrap();

    let artist2 = media.upsert_artist("隐藏歌手").await.unwrap();
    let album2 = media
        .upsert_album("隐藏专辑", Some(artist2), None, None)
        .await
        .unwrap();
    media
        .upsert_track(&common::track(
            "隐藏曲目",
            Some(album2),
            Some(artist2),
            "b/hidden.flac",
        ))
        .await
        .unwrap();

    let grant_alice = Principal {
        principal_type: PrincipalType::User,
        id: alice.to_string(),
    };
    // 曲目级：机密曲目仅 alice。专辑级：整张隐藏专辑仅 alice。
    ctx.index
        .access()
        .set_rule(
            ScopeType::Track,
            &secret_track.to_string(),
            None,
            std::slice::from_ref(&grant_alice),
        )
        .await
        .unwrap();
    ctx.index
        .access()
        .set_rule(ScopeType::Album, &album2.to_string(), None, &[grant_alice])
        .await
        .unwrap();

    Fixture {
        alice,
        bob,
        root,
        artist1,
        artist2,
        album1,
        album2,
        open_track,
        secret_track,
    }
}

#[tokio::test]
async fn get_song_受限曲目对无授权者遮蔽对授权者与管理员可见() {
    let ctx = common::ctx().await;
    let f = seed(&ctx).await;
    let uri = format!("/rest/getSong?id={}&f=json", tr(f.secret_track));

    let (_, bob) = ctx.get_json(&uri, Some(&ctx.bearer(f.bob))).await;
    assert_eq!(bob["subsonic-response"]["status"], "failed");
    assert_eq!(bob["subsonic-response"]["error"]["code"], 70);
    assert!(bob["subsonic-response"]["song"].is_null(), "不得泄漏曲目");

    let (_, alice) = ctx.get_json(&uri, Some(&ctx.bearer(f.alice))).await;
    assert_eq!(alice["subsonic-response"]["status"], "ok");
    assert_eq!(alice["subsonic-response"]["song"]["id"], tr(f.secret_track));

    let (_, root) = ctx.get_json(&uri, Some(&ctx.bearer(f.root))).await;
    assert_eq!(root["subsonic-response"]["song"]["id"], tr(f.secret_track));
}

#[tokio::test]
async fn get_album_歌曲列表按可见性过滤() {
    let ctx = common::ctx().await;
    let f = seed(&ctx).await;
    let uri = format!("/rest/getAlbum?id={}&f=json", al(f.album1));

    let (_, bob) = ctx.get_json(&uri, Some(&ctx.bearer(f.bob))).await;
    let bob_songs = ids(&bob["subsonic-response"]["album"]["song"]);
    assert_eq!(bob_songs, vec![tr(f.open_track)], "bob 只见公开曲目");

    let (_, alice) = ctx.get_json(&uri, Some(&ctx.bearer(f.alice))).await;
    let mut alice_songs = ids(&alice["subsonic-response"]["album"]["song"]);
    alice_songs.sort();
    let mut want = vec![tr(f.open_track), tr(f.secret_track)];
    want.sort();
    assert_eq!(alice_songs, want, "alice 见全部曲目");
}

#[tokio::test]
async fn get_artists_隐藏无可见曲目的艺人() {
    let ctx = common::ctx().await;
    let f = seed(&ctx).await;

    let (_, bob) = ctx
        .get_json("/rest/getArtists?f=json", Some(&ctx.bearer(f.bob)))
        .await;
    let bob_ids = artist_ids(&bob, "artists");
    assert!(bob_ids.contains(&ar(f.artist1)), "有公开曲目的艺人可见");
    assert!(!bob_ids.contains(&ar(f.artist2)), "全受限艺人对 bob 不可见");

    let (_, alice) = ctx
        .get_json("/rest/getArtists?f=json", Some(&ctx.bearer(f.alice)))
        .await;
    let alice_ids = artist_ids(&alice, "artists");
    assert!(alice_ids.contains(&ar(f.artist2)), "alice 见受限艺人");
}

#[tokio::test]
async fn get_indexes_与_get_artists_同样过滤() {
    let ctx = common::ctx().await;
    let f = seed(&ctx).await;
    let (_, bob) = ctx
        .get_json("/rest/getIndexes?f=json", Some(&ctx.bearer(f.bob)))
        .await;
    let bob_ids = artist_ids(&bob, "indexes");
    assert!(!bob_ids.contains(&ar(f.artist2)));
}

#[tokio::test]
async fn get_album_list2_隐藏无可见曲目的专辑() {
    let ctx = common::ctx().await;
    let f = seed(&ctx).await;

    let (_, bob) = ctx
        .get_json(
            "/rest/getAlbumList2?type=alphabeticalByName&f=json&size=50",
            Some(&ctx.bearer(f.bob)),
        )
        .await;
    let bob_albums = ids(&bob["subsonic-response"]["albumList2"]["album"]);
    assert!(bob_albums.contains(&al(f.album1)));
    assert!(!bob_albums.contains(&al(f.album2)), "隐藏专辑不出现");

    let (_, alice) = ctx
        .get_json(
            "/rest/getAlbumList2?type=alphabeticalByName&f=json&size=50",
            Some(&ctx.bearer(f.alice)),
        )
        .await;
    let alice_albums = ids(&alice["subsonic-response"]["albumList2"]["album"]);
    assert!(alice_albums.contains(&al(f.album2)));
}

#[tokio::test]
async fn search3_过滤受限命中() {
    let ctx = common::ctx().await;
    let f = seed(&ctx).await;
    let uri = "/rest/search3?query=机密曲&f=json";

    let (_, bob) = ctx.get_json(uri, Some(&ctx.bearer(f.bob))).await;
    let bob_songs = ids(&bob["subsonic-response"]["searchResult3"]["song"]);
    assert!(bob_songs.is_empty(), "受限曲目不出现在 bob 搜索结果");

    let (_, alice) = ctx.get_json(uri, Some(&ctx.bearer(f.alice))).await;
    let alice_songs = ids(&alice["subsonic-response"]["searchResult3"]["song"]);
    assert_eq!(alice_songs, vec![tr(f.secret_track)], "alice 可搜到");
}

#[tokio::test]
async fn 未认证请求被拒() {
    let ctx = common::ctx().await;
    let f = seed(&ctx).await;
    let (status, body) = ctx
        .get_json(
            &format!("/rest/getSong?id={}&f=json", tr(f.open_track)),
            None,
        )
        .await;
    // OpenSubsonic 惯例：认证失败为 HTTP 200 + subsonic 错误码（缺凭证=10），非 401。
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(body["subsonic-response"]["status"], "failed");
    assert_eq!(body["subsonic-response"]["error"]["code"], 10);
}

#[tokio::test]
async fn get_album_默认返回_xml() {
    let ctx = common::ctx().await;
    let f = seed(&ctx).await;
    let (status, body) = ctx
        .get(
            &format!("/rest/getAlbum?id={}", al(f.album1)),
            Some(&ctx.bearer(f.alice)),
        )
        .await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert!(body.contains("<subsonic-response"), "XML 根元素: {body}");
    assert!(body.contains("<album"), "含 album 元素: {body}");
    assert!(body.contains("<song"), "含 song 子元素: {body}");
}
