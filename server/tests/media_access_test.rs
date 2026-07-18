//! 端到端：媒体端点 `stream`/`download`/`getCoverArt` 的曲库访问控制门控（计划 T9）。
//!
//! 无授权用户对受限曲目「放不了」——以 subsonic 错误 70 遮蔽；授权者/管理员可取到原始字节。

mod common;

use bytes::Bytes;
use contract::{Principal, PrincipalType, ScopeType};
use yevune_server::storage::ObjectStore;

/// 播种：一张 album1（公开曲目 open + 受限曲目 secret）与全受限 album2；音频与封面写入存储。
struct Media {
    alice: i64,
    bob: i64,
    open_track: i64,
    secret_track: i64,
    album1_cover: String,
    album2_cover: String,
}

async fn seed(ctx: &common::Ctx) -> Media {
    let alice = ctx.create_user("alice", &[]).await;
    let bob = ctx.create_user("bob", &[]).await;
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

    // 设封面键并写入封面字节。
    let album1_cover = "covers/al1.jpg".to_string();
    let album2_cover = "covers/al2.jpg".to_string();
    sqlx::query("UPDATE albums SET cover_key = ? WHERE id = ?")
        .bind(&album1_cover)
        .bind(album1)
        .execute(ctx.index.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE albums SET cover_key = ? WHERE id = ?")
        .bind(&album2_cover)
        .bind(album2)
        .execute(ctx.index.pool())
        .await
        .unwrap();

    // 写入音频与封面对象。
    ctx.store
        .put("a/open.flac", Bytes::from_static(b"OPEN-AUDIO-BYTES"))
        .await
        .unwrap();
    ctx.store
        .put("a/secret.flac", Bytes::from_static(b"SECRET-AUDIO-BYTES"))
        .await
        .unwrap();
    ctx.store
        .put(&album1_cover, Bytes::from_static(b"IMG1"))
        .await
        .unwrap();
    ctx.store
        .put(&album2_cover, Bytes::from_static(b"IMG2"))
        .await
        .unwrap();

    let grant_alice = Principal {
        principal_type: PrincipalType::User,
        id: alice.to_string(),
    };
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

    Media {
        alice,
        bob,
        open_track,
        secret_track,
        album1_cover,
        album2_cover,
    }
}

#[tokio::test]
async fn stream_受限曲目对无授权者遮蔽() {
    let ctx = common::ctx().await;
    let m = seed(&ctx).await;
    let uri = format!("/rest/stream?id={}", m.secret_track);

    // bob 无授权：以 subsonic 错误 70 遮蔽，不返回音频字节。
    let (status, body) = ctx.get(&uri, Some(&ctx.bearer(m.bob))).await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert!(body.contains("status=\"failed\""), "应为失败信封: {body}");
    assert!(body.contains("code=\"70\""), "应为未找到: {body}");
    assert!(!body.contains("SECRET-AUDIO"), "不得泄漏音频字节");
}

#[tokio::test]
async fn lyrics_受限曲目对无授权者遮蔽() {
    let ctx = common::ctx().await;
    let m = seed(&ctx).await;
    let uri = format!("/rest/getLyricsBySongId?id={}&f=json", m.secret_track);

    let (_, body) = ctx.get(&uri, Some(&ctx.bearer(m.bob))).await;

    assert!(
        body.contains("\"status\":\"failed\""),
        "应为失败信封: {body}"
    );
    assert!(
        body.contains("\"code\":70"),
        "受限歌词应按未找到遮蔽: {body}"
    );
    assert!(
        !body.contains("structuredLyrics"),
        "不得泄漏歌词结构: {body}"
    );
}

#[tokio::test]
async fn stream_授权者取到原始音频字节() {
    let ctx = common::ctx().await;
    let m = seed(&ctx).await;
    let uri = format!("/rest/stream?id={}", m.secret_track);

    let (status, body) = ctx.get(&uri, Some(&ctx.bearer(m.alice))).await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(body, "SECRET-AUDIO-BYTES", "授权者透传原始字节");
}

#[tokio::test]
async fn stream_公开曲目对所有用户可播() {
    let ctx = common::ctx().await;
    let m = seed(&ctx).await;
    let uri = format!("/rest/stream?id={}", m.open_track);
    let (status, body) = ctx.get(&uri, Some(&ctx.bearer(m.bob))).await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(body, "OPEN-AUDIO-BYTES");
}

#[tokio::test]
async fn download_同样门控且授权者取到字节() {
    let ctx = common::ctx().await;
    let m = seed(&ctx).await;
    let uri = format!("/rest/download?id={}", m.secret_track);

    let (_, bob) = ctx.get(&uri, Some(&ctx.bearer(m.bob))).await;
    assert!(bob.contains("code=\"70\""), "bob 被遮蔽: {bob}");

    let (status, alice) = ctx.get(&uri, Some(&ctx.bearer(m.alice))).await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(alice, "SECRET-AUDIO-BYTES");
}

#[tokio::test]
async fn 有效流派规则覆盖_stream_与_download() {
    let ctx = common::ctx().await;
    let bob = ctx.create_user("bob", &[]).await;
    let root = ctx.create_user("root", &["admin"]).await;
    let mut track = common::track("覆盖流派音频", None, None, "genre/audio.flac");
    track.genre = Some("Rock".into());
    let track_id = ctx.index.media().upsert_track(&track).await.unwrap();
    ctx.index
        .media()
        .set_tag_overrides(track_id, &[("genre", Some("Kids"))])
        .await
        .unwrap();
    ctx.store
        .put("genre/audio.flac", Bytes::from_static(b"GENRE-AUDIO"))
        .await
        .unwrap();
    ctx.index
        .access()
        .set_rule(ScopeType::Genre, "Kids", None, &[])
        .await
        .unwrap();

    for endpoint in ["stream", "download"] {
        let uri = format!("/rest/{endpoint}?id={track_id}");
        let (_, denied) = ctx.get(&uri, Some(&ctx.bearer(bob))).await;
        assert!(
            denied.contains("code=\"70\""),
            "{endpoint} 必须拒绝未授权普通用户: {denied}"
        );
        let (_, admin) = ctx.get(&uri, Some(&ctx.bearer(root))).await;
        assert_eq!(admin, "GENRE-AUDIO", "管理员可通过 {endpoint}");
    }

    ctx.index
        .access()
        .set_rule(
            ScopeType::Genre,
            "Kids",
            None,
            &[Principal {
                principal_type: PrincipalType::User,
                id: bob.to_string(),
            }],
        )
        .await
        .unwrap();
    for endpoint in ["stream", "download"] {
        let uri = format!("/rest/{endpoint}?id={track_id}");
        let (_, granted) = ctx.get(&uri, Some(&ctx.bearer(bob))).await;
        assert_eq!(granted, "GENRE-AUDIO", "用户授权后可通过 {endpoint}");
    }
}

#[tokio::test]
async fn get_cover_art_按封面归属可见性门控() {
    let ctx = common::ctx().await;
    let m = seed(&ctx).await;

    // album1 含对 bob 可见的公开曲目 → 封面可取。
    let (status, body) = ctx
        .get(
            &format!("/rest/getCoverArt?id={}", m.album1_cover),
            Some(&ctx.bearer(m.bob)),
        )
        .await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(body, "IMG1", "bob 可取部分可见专辑的封面");

    // album2 对 bob 全受限 → 封面被遮蔽。
    let (_, hidden) = ctx
        .get(
            &format!("/rest/getCoverArt?id={}", m.album2_cover),
            Some(&ctx.bearer(m.bob)),
        )
        .await;
    assert!(hidden.contains("code=\"70\""), "受限封面被遮蔽: {hidden}");

    // alice 可取受限专辑封面。
    let (status, alice) = ctx
        .get(
            &format!("/rest/getCoverArt?id={}", m.album2_cover),
            Some(&ctx.bearer(m.alice)),
        )
        .await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(alice, "IMG2");
}

#[tokio::test]
async fn stream_未认证被拒() {
    let ctx = common::ctx().await;
    let m = seed(&ctx).await;
    let (status, body) = ctx
        .get_json(&format!("/rest/stream?id={}&f=json", m.open_track), None)
        .await;
    // OpenSubsonic 惯例：认证失败以 HTTP 200 + subsonic 错误码回应（缺凭证=10），
    // 而非 HTTP 401——现成客户端读响应信封里的错误码。
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(body["subsonic-response"]["status"], "failed");
    assert_eq!(body["subsonic-response"]["error"]["code"], 10);
}
