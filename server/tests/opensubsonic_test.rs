//! OpenSubsonic API 一致性集成测试。

use std::ops::Range;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use bytes::Bytes;
use http_body_util::BodyExt;
use lofty::config::WriteOptions;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::prelude::ItemKey;
use lofty::probe::Probe;
use tower::ServiceExt;
use yevune_server::api::AppState;
use yevune_server::auth::{Encryptor, UserAdmin};
use yevune_server::index::{Index, NewTrack};
use yevune_server::storage::{
    ListPage, MemoryStore, ObjectMeta, ObjectStore, Result as StoreResult,
};

struct Fixture {
    state: AppState,
    index: Index,
    store: Arc<MemoryStore>,
    admin_id: i64,
    artist_id: i64,
    album_id: i64,
    track_id: i64,
    playlist_id: i64,
    _dir: tempfile::TempDir,
}

fn lyric_flac() -> Bytes {
    let temp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(temp.path(), include_bytes!("fixtures/scanner/a.flac")).unwrap();
    let mut tagged = Probe::open(temp.path())
        .unwrap()
        .guess_file_type()
        .unwrap()
        .read()
        .unwrap();
    tagged.primary_tag_mut().unwrap().insert_text(
        ItemKey::Lyrics,
        "[offset:-100]\n[00:00.50]First line\n[00:02.00]Second line".into(),
    );
    tagged
        .save_to_path(temp.path(), WriteOptions::default())
        .unwrap();
    Bytes::from(std::fs::read(temp.path()).unwrap())
}

impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let index = Index::connect(&dir.path().join("yevune.sqlite"))
            .await
            .unwrap();
        let encryptor = Encryptor::new("pwd:test-secret");
        let admin = UserAdmin::new(&index, &encryptor);
        let admin_user = admin.create_user("admin", "secret", true).await.unwrap();
        admin.create_user("member", "secret", false).await.unwrap();
        let admin_id = admin_user.id.parse().unwrap();

        let artist_id = index.media().upsert_artist("Test Artist").await.unwrap();
        let album_id = index
            .media()
            .upsert_album("Test Album", Some(artist_id), Some(2026), Some("Rock"))
            .await
            .unwrap();
        let track_id = index
            .media()
            .upsert_track(&NewTrack {
                title: "Test Song".into(),
                album_id: Some(album_id),
                artist_id: Some(artist_id),
                track_no: Some(1),
                year: Some(2026),
                genre: Some("Rock".into()),
                duration: Some(180),
                codec: Some("flac".into()),
                bitrate: Some(900),
                size: Some(11),
                object_key: "library/test.flac".into(),
                ..NewTrack::default()
            })
            .await
            .unwrap();
        sqlx::query("UPDATE albums SET cover_key = 'covers/test.jpg' WHERE id = ?")
            .bind(album_id)
            .execute(index.pool())
            .await
            .unwrap();
        let playlist_id = index
            .playlists()
            .create_playlist(admin_id, "Favorites", None)
            .await
            .unwrap();
        index
            .playlists()
            .set_tracks(playlist_id, &[track_id])
            .await
            .unwrap();

        let store = Arc::new(MemoryStore::new());
        store
            .put(
                "library/test.flac",
                bytes::Bytes::from_static(b"hello-audio"),
            )
            .await
            .unwrap();
        store
            .put("covers/test.jpg", bytes::Bytes::from_static(b"fake-jpeg"))
            .await
            .unwrap();
        let object_store: Arc<dyn ObjectStore> = store.clone();
        let state = AppState::new(
            index.clone(),
            object_store,
            "test-secret",
            "/missing/ffmpeg",
        );
        Self {
            state,
            index,
            store,
            admin_id,
            artist_id,
            album_id,
            track_id,
            playlist_id,
            _dir: dir,
        }
    }

    fn uri(&self, path: &str) -> String {
        let separator = if path.contains('?') { '&' } else { '?' };
        format!("{path}{separator}u=admin&p=secret&v=1.16.1&c=test")
    }

    async fn get(&self, uri: &str) -> axum::response::Response {
        yevune_server::app(self.state.clone())
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap()
    }
}

#[tokio::test]
async fn get_lyrics_by_song_id_returns_standard_synced_lyrics() {
    let fixture = Fixture::new().await;
    let audio = lyric_flac();
    fixture
        .store
        .put("library/test.flac", audio.clone())
        .await
        .unwrap();
    sqlx::query("UPDATE tracks SET size = ? WHERE id = ?")
        .bind(audio.len() as i64)
        .bind(fixture.track_id)
        .execute(fixture.index.pool())
        .await
        .unwrap();
    let response = fixture
        .get(&fixture.uri(&format!(
            "/rest/getLyricsBySongId.view?id=tr-{}&f=json",
            fixture.track_id
        )))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let json = json_body(response).await;
    let lyrics = &json["subsonic-response"]["lyricsList"]["structuredLyrics"][0];
    assert_eq!(lyrics["displayArtist"], "Test Artist");
    assert_eq!(lyrics["displayTitle"], "Test Song");
    assert_eq!(lyrics["offset"], -100);
    assert_eq!(lyrics["synced"], true);
    assert_eq!(lyrics["line"][0]["start"], 500);
    assert_eq!(lyrics["line"][0]["value"], "First line");
    assert_eq!(lyrics["line"][1]["start"], 2_000);
}

async fn json_body(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

async fn text_body(response: axum::response::Response) -> String {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

fn assert_rfc3339_utc(value: &serde_json::Value) -> chrono::DateTime<chrono::FixedOffset> {
    let text = value.as_str().expect("时间字段必须是字符串");
    assert!(text.ends_with('Z'), "UTC 时间必须使用 Z 后缀: {text}");
    let parsed = chrono::DateTime::parse_from_rfc3339(text).expect("时间字段必须可按 RFC3339 解析");
    assert_eq!(parsed.offset().local_minus_utc(), 0);
    parsed
}

#[tokio::test]
async fn system_endpoints_use_exact_envelope_names_and_view_aliases() {
    let fixture = Fixture::new().await;
    let cases = [
        ("/rest/ping?f=json", None),
        ("/rest/ping.view?f=json", None),
        ("/rest/getLicense?f=json", Some("license")),
        (
            "/rest/getOpenSubsonicExtensions.view?f=json",
            Some("openSubsonicExtensions"),
        ),
    ];

    for (path, nested) in cases {
        let response = fixture.get(&fixture.uri(path)).await;
        assert_eq!(response.status(), StatusCode::OK, "{path}");
        let json = json_body(response).await;
        let envelope = &json["subsonic-response"];
        assert_eq!(envelope["status"], "ok", "{path}: {json}");
        assert_eq!(envelope["version"], "1.16.1", "{path}: {json}");
        assert_eq!(envelope["openSubsonic"], true, "{path}: {json}");
        if let Some(name) = nested {
            assert!(envelope.get(name).is_some(), "{path} 缺少 {name}: {json}");
        }
        if path.contains("getOpenSubsonicExtensions") {
            assert!(
                envelope["openSubsonicExtensions"].is_array(),
                "{path}: {json}"
            );
        }
    }
}

#[tokio::test]
async fn extensions_is_public_but_other_endpoints_require_authentication() {
    let fixture = Fixture::new().await;
    let public = fixture.get("/rest/getOpenSubsonicExtensions?f=json").await;
    assert_eq!(public.status(), StatusCode::OK);

    let response = fixture.get("/rest/getLicense?f=json").await;
    assert_eq!(response.status(), StatusCode::OK);
    let json = json_body(response).await;
    assert_eq!(json["subsonic-response"]["status"], "failed");
    assert_eq!(json["subsonic-response"]["error"]["code"], 10);
}

#[tokio::test]
async fn wrong_credentials_return_protocol_error_without_internal_details() {
    let fixture = Fixture::new().await;
    let response = fixture
        .get("/rest/ping?u=admin&p=wrong&v=1.16.1&c=test&f=json")
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let json = json_body(response).await;
    assert_eq!(json["subsonic-response"]["status"], "failed");
    assert_eq!(json["subsonic-response"]["error"]["code"], 40);
    assert!(!json.to_string().contains("存储错误"));
}

#[tokio::test]
async fn default_xml_has_standard_root_and_nested_license() {
    let fixture = Fixture::new().await;
    let response = fixture.get(&fixture.uri("/rest/getLicense")).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers()[header::CONTENT_TYPE]
        .to_str()
        .unwrap()
        .contains("xml"));
    let xml = text_body(response).await;
    assert!(xml.contains("<subsonic-response"), "{xml}");
    assert!(xml.contains("status=\"ok\""), "{xml}");
    assert!(xml.contains("<license"), "{xml}");
}

#[tokio::test]
async fn browsing_and_search_endpoints_use_required_nested_names() {
    let fixture = Fixture::new().await;
    let cases = [
        ("/rest/getArtists?f=json".to_string(), "artists"),
        (
            format!("/rest/getArtist.view?id=ar-{}&f=json", fixture.artist_id),
            "artist",
        ),
        (
            format!("/rest/getAlbum?id=al-{}&f=json", fixture.album_id),
            "album",
        ),
        (
            format!("/rest/getSong?id={}&f=json", fixture.track_id),
            "song",
        ),
        (
            "/rest/getAlbumList2?type=newest&size=10&offset=0&f=json".to_string(),
            "albumList2",
        ),
        ("/rest/getGenres?f=json".to_string(), "genres"),
        ("/rest/getIndexes?f=json".to_string(), "indexes"),
        (
            "/rest/search3?query=Test&artistCount=5&albumCount=5&songCount=5&f=json".to_string(),
            "searchResult3",
        ),
    ];

    for (path, nested) in cases {
        let response = fixture.get(&fixture.uri(&path)).await;
        assert_eq!(response.status(), StatusCode::OK, "{path}");
        let json = json_body(response).await;
        assert_eq!(json["subsonic-response"]["status"], "ok", "{path}: {json}");
        assert!(
            json["subsonic-response"].get(nested).is_some(),
            "{path} 缺少 {nested}: {json}"
        );
    }
}

#[tokio::test]
async fn browsing_payloads_include_related_children_and_genre_counts() {
    let fixture = Fixture::new().await;

    let artist = json_body(
        fixture
            .get(&fixture.uri(&format!(
                "/rest/getArtist?id=ar-{}&f=json",
                fixture.artist_id
            )))
            .await,
    )
    .await;
    assert_eq!(
        artist["subsonic-response"]["artist"]["album"][0]["id"],
        format!("al-{}", fixture.album_id)
    );

    let album = json_body(
        fixture
            .get(&fixture.uri(&format!("/rest/getAlbum?id=al-{}&f=json", fixture.album_id)))
            .await,
    )
    .await;
    assert_eq!(
        album["subsonic-response"]["album"]["song"][0]["id"],
        format!("tr-{}", fixture.track_id)
    );

    let genres = json_body(fixture.get(&fixture.uri("/rest/getGenres?f=json")).await).await;
    assert_eq!(
        genres["subsonic-response"]["genres"]["genre"][0]["value"],
        "Rock"
    );
    assert_eq!(
        genres["subsonic-response"]["genres"]["genre"][0]["songCount"],
        1
    );
}

#[tokio::test]
async fn missing_and_unknown_ids_return_protocol_errors() {
    let fixture = Fixture::new().await;
    let missing = json_body(fixture.get(&fixture.uri("/rest/getSong?f=json")).await).await;
    assert_eq!(missing["subsonic-response"]["error"]["code"], 10);

    let unknown = json_body(
        fixture
            .get(&fixture.uri("/rest/getSong?id=999999&f=json"))
            .await,
    )
    .await;
    assert_eq!(unknown["subsonic-response"]["error"]["code"], 70);
}

#[tokio::test]
async fn playlist_crud_is_owner_scoped_and_uses_playlist_nesting() {
    let fixture = Fixture::new().await;
    let list = json_body(fixture.get(&fixture.uri("/rest/getPlaylists?f=json")).await).await;
    assert_eq!(
        list["subsonic-response"]["playlists"]["playlist"][0]["name"],
        "Favorites"
    );
    assert!(list["subsonic-response"]["playlists"]["playlist"][0]["created"].is_string());
    assert!(list["subsonic-response"]["playlists"]["playlist"][0]["changed"].is_string());
    assert_rfc3339_utc(&list["subsonic-response"]["playlists"]["playlist"][0]["created"]);
    assert_rfc3339_utc(&list["subsonic-response"]["playlists"]["playlist"][0]["changed"]);

    let detail = json_body(
        fixture
            .get(&fixture.uri(&format!(
                "/rest/getPlaylist?id=pl-{}&f=json",
                fixture.playlist_id
            )))
            .await,
    )
    .await;
    assert_eq!(
        detail["subsonic-response"]["playlist"]["entry"][0]["id"],
        format!("tr-{}", fixture.track_id)
    );

    let created = json_body(
        fixture
            .get(&fixture.uri(&format!(
                "/rest/createPlaylist?name=Road&songId={}&f=json",
                fixture.track_id
            )))
            .await,
    )
    .await;
    let created_id = created["subsonic-response"]["playlist"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_rfc3339_utc(&created["subsonic-response"]["playlist"]["created"]);
    assert_rfc3339_utc(&created["subsonic-response"]["playlist"]["changed"]);
    let created_raw_id: i64 = created_id.strip_prefix("pl-").unwrap().parse().unwrap();
    sqlx::query("UPDATE playlists SET changed_at = '2000-01-01 00:00:00' WHERE id = ?")
        .bind(created_raw_id)
        .execute(fixture.index.pool())
        .await
        .unwrap();

    let updated = fixture
        .get(&fixture.uri(&format!(
            "/rest/updatePlaylist?playlistId={created_id}&name=Renamed&f=json"
        )))
        .await;
    assert_eq!(
        json_body(updated).await["subsonic-response"]["status"],
        "ok"
    );
    let updated = json_body(
        fixture
            .get(&fixture.uri(&format!("/rest/getPlaylist?id={created_id}&f=json")))
            .await,
    )
    .await;
    assert_ne!(
        updated["subsonic-response"]["playlist"]["changed"],
        "2000-01-01T00:00:00Z"
    );
    let changed = assert_rfc3339_utc(&updated["subsonic-response"]["playlist"]["changed"]);
    assert!(changed > chrono::DateTime::parse_from_rfc3339("2000-01-01T00:00:00Z").unwrap());

    let deleted = fixture
        .get(&fixture.uri(&format!("/rest/deletePlaylist?id={created_id}&f=json")))
        .await;
    assert_eq!(
        json_body(deleted).await["subsonic-response"]["status"],
        "ok"
    );
}

#[tokio::test]
async fn annotation_endpoints_persist_per_user_state() {
    let fixture = Fixture::new().await;
    for path in [
        format!("/rest/star?id={}&f=json", fixture.track_id),
        format!("/rest/unstar?id={}&f=json", fixture.track_id),
        format!("/rest/setRating?id={}&rating=4&f=json", fixture.track_id),
        format!(
            "/rest/scrobble?id={}&submission=true&f=json",
            fixture.track_id
        ),
    ] {
        let body = json_body(fixture.get(&fixture.uri(&path)).await).await;
        assert_eq!(body["subsonic-response"]["status"], "ok", "{path}: {body}");
    }
    let annotation = fixture
        .index
        .annotations()
        .get(fixture.admin_id, "track", fixture.track_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(annotation.rating, Some(4));
    assert_eq!(annotation.play_count, 1);
    assert!(annotation.starred_at.is_none());
}

#[tokio::test]
async fn media_endpoints_return_binary_without_protocol_envelope() {
    let fixture = Fixture::new().await;
    let cases = [
        format!("/rest/stream.view?id={}&format=raw", fixture.track_id),
        format!("/rest/download?id={}", fixture.track_id),
        "/rest/getCoverArt?id=covers%2Ftest.jpg".to_string(),
    ];
    for path in cases {
        let response = fixture.get(&fixture.uri(&path)).await;
        assert_eq!(response.status(), StatusCode::OK, "{path}");
        let content_type = response.headers()[header::CONTENT_TYPE]
            .to_str()
            .unwrap()
            .to_string();
        assert!(!content_type.contains("json"), "{path}: {content_type}");
        assert!(!content_type.contains("xml"), "{path}: {content_type}");
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        assert!(!bytes.is_empty(), "{path} 应返回二进制内容");
        assert!(!bytes.starts_with(b"<subsonic-response"));
    }
}

#[tokio::test]
async fn media_errors_use_protocol_envelope_and_do_not_leak_storage_details() {
    let fixture = Fixture::new().await;
    let response = fixture
        .get(&fixture.uri("/rest/download?id=999999&f=json"))
        .await;
    let json = json_body(response).await;
    assert_eq!(json["subsonic-response"]["status"], "failed");
    assert_eq!(json["subsonic-response"]["error"]["code"], 70);
    assert!(!json.to_string().contains("object_key"));

    let missing_object_id = fixture
        .index
        .media()
        .upsert_track(&NewTrack {
            title: "Missing Object".into(),
            codec: Some("flac".into()),
            object_key: "library/missing.flac".into(),
            ..NewTrack::default()
        })
        .await
        .unwrap();
    let response = fixture
        .get(&fixture.uri(&format!("/rest/download?id=tr-{missing_object_id}&f=json")))
        .await;
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "application/json",
        "媒体对象缺失也必须遵循请求的协议信封格式"
    );
    let json = json_body(response).await;
    assert_eq!(json["subsonic-response"]["error"]["code"], 70);
}

#[tokio::test]
async fn scan_endpoints_report_status_and_enforce_admin_start() {
    let fixture = Fixture::new().await;
    let status = json_body(
        fixture
            .get(&fixture.uri("/rest/getScanStatus.view?f=json"))
            .await,
    )
    .await;
    assert!(status["subsonic-response"]["scanStatus"]["scanning"].is_boolean());

    let started = json_body(fixture.get(&fixture.uri("/rest/startScan?f=json")).await).await;
    assert_eq!(started["subsonic-response"]["status"], "ok");
    assert!(started["subsonic-response"]["scanStatus"].is_object());
    assert_eq!(started["subsonic-response"]["scanStatus"]["scanning"], true);

    let denied = json_body(
        fixture
            .get("/rest/startScan?u=member&p=secret&v=1.16.1&c=test&f=json")
            .await,
    )
    .await;
    assert_eq!(denied["subsonic-response"]["error"]["code"], 50);
}

#[tokio::test]
async fn user_read_endpoints_use_exact_nested_names() {
    let fixture = Fixture::new().await;
    let user = json_body(
        fixture
            .get(&fixture.uri("/rest/getUser.view?username=member&f=json"))
            .await,
    )
    .await;
    assert_eq!(user["subsonic-response"]["user"]["username"], "member");
    assert_eq!(user["subsonic-response"]["user"]["adminRole"], false);

    let users = json_body(fixture.get(&fixture.uri("/rest/getUsers?f=json")).await).await;
    assert!(users["subsonic-response"]["users"]["user"].is_array());
    assert_eq!(
        users["subsonic-response"]["users"]["user"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn admin_can_create_update_change_password_and_delete_user() {
    let fixture = Fixture::new().await;
    for path in [
        "/rest/createUser?username=child&password=pw&email=child%40example.test&f=json",
        "/rest/updateUser?username=child&password=pw2&adminRole=false&f=json",
        "/rest/changePassword?username=child&password=pw3&f=json",
    ] {
        let body = json_body(fixture.get(&fixture.uri(path)).await).await;
        assert_eq!(body["subsonic-response"]["status"], "ok", "{path}: {body}");
    }
    assert_eq!(
        fixture
            .index
            .users()
            .get_user_by_name("child")
            .await
            .unwrap()
            .unwrap()
            .email
            .as_deref(),
        Some("child@example.test")
    );
    let authenticated = fixture
        .get("/rest/ping?u=child&p=pw3&v=1.16.1&c=test&f=json")
        .await;
    assert_eq!(
        json_body(authenticated).await["subsonic-response"]["status"],
        "ok"
    );

    let deleted = json_body(
        fixture
            .get(&fixture.uri("/rest/deleteUser.view?username=child&f=json"))
            .await,
    )
    .await;
    assert_eq!(deleted["subsonic-response"]["status"], "ok");
    assert!(fixture
        .index
        .users()
        .get_user_by_name("child")
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn user_management_is_admin_only_and_validates_required_parameters() {
    let fixture = Fixture::new().await;
    let denied = json_body(
        fixture
            .get("/rest/getUsers?u=member&p=secret&v=1.16.1&c=test&f=json")
            .await,
    )
    .await;
    assert_eq!(denied["subsonic-response"]["error"]["code"], 50);

    let missing = json_body(
        fixture
            .get(&fixture.uri("/rest/createUser?username=child&password=pw&f=json"))
            .await,
    )
    .await;
    assert_eq!(missing["subsonic-response"]["error"]["code"], 10);
}

#[tokio::test]
async fn malformed_parameters_always_return_protocol_envelope() {
    let fixture = Fixture::new().await;
    for path in [
        "/rest/getSong?id=not-a-number&f=json",
        "/rest/stream?id=not-a-number&f=json",
        "/rest/setRating?id=1&rating=99x&f=json",
    ] {
        let response = fixture.get(&fixture.uri(path)).await;
        assert_eq!(response.status(), StatusCode::OK, "{path}");
        let body = json_body(response).await;
        assert_eq!(
            body["subsonic-response"]["status"], "failed",
            "{path}: {body}"
        );
        assert_eq!(
            body["subsonic-response"]["error"]["code"], 10,
            "{path}: {body}"
        );
    }
}

#[tokio::test]
async fn repeated_playlist_and_annotation_ids_are_accepted() {
    let fixture = Fixture::new().await;
    let created = json_body(
        fixture
            .get(&fixture.uri(&format!(
                "/rest/createPlaylist?name=Repeated&songId={0}&songId={0}&f=json",
                fixture.track_id
            )))
            .await,
    )
    .await;
    assert_eq!(
        created["subsonic-response"]["playlist"]["entry"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    let scrobbled = json_body(
        fixture
            .get(&fixture.uri(&format!(
                "/rest/scrobble?id={0}&id={0}&submission=true&f=json",
                fixture.track_id
            )))
            .await,
    )
    .await;
    assert_eq!(scrobbled["subsonic-response"]["status"], "ok");
    let annotation = fixture
        .index
        .annotations()
        .get(fixture.admin_id, "track", fixture.track_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(annotation.play_count, 2);
}

#[tokio::test]
async fn direct_media_honors_single_http_range_without_buffering_whole_object() {
    let fixture = Fixture::new().await;
    for path in [
        format!("/rest/stream?id={}&format=raw", fixture.track_id),
        format!("/rest/download?id={}", fixture.track_id),
    ] {
        let response = yevune_server::app(fixture.state.clone())
            .oneshot(
                Request::builder()
                    .uri(fixture.uri(&path))
                    .header(header::RANGE, "bytes=1-4")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT, "{path}");
        assert_eq!(response.headers()[header::CONTENT_RANGE], "bytes 1-4/11");
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&bytes[..], b"ello", "{path}");
    }
}

#[tokio::test]
async fn cover_art_id_cannot_read_arbitrary_audio_or_transcode_objects() {
    let fixture = Fixture::new().await;
    let body = json_body(
        fixture
            .get(&fixture.uri("/rest/getCoverArt?id=music%2Ftest.flac&f=json"))
            .await,
    )
    .await;
    assert_eq!(body["subsonic-response"]["error"]["code"], 70);
}

fn encoded_cover(format: image::ImageFormat) -> Vec<u8> {
    if format == image::ImageFormat::Gif {
        let mut bytes = Vec::new();
        let mut encoder = image::codecs::gif::GifEncoder::new(&mut bytes);
        let frames = [
            image::Frame::new(image::RgbaImage::from_pixel(
                40,
                20,
                image::Rgba([20, 100, 200, 255]),
            )),
            image::Frame::new(image::RgbaImage::from_pixel(
                40,
                20,
                image::Rgba([200, 100, 20, 255]),
            )),
        ];
        encoder.encode_frames(frames).unwrap();
        drop(encoder);
        return bytes;
    }
    let image = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
        40,
        20,
        image::Rgb([20, 100, 200]),
    ));
    let mut bytes = std::io::Cursor::new(Vec::new());
    image.write_to(&mut bytes, format).unwrap();
    bytes.into_inner()
}

#[tokio::test]
async fn cover_art_size_resizes_all_scanner_formats_and_preserves_original_without_size() {
    let fixture = Fixture::new().await;
    for (suffix, format, mime) in [
        ("jpg", image::ImageFormat::Jpeg, "image/jpeg"),
        ("png", image::ImageFormat::Png, "image/png"),
        ("gif", image::ImageFormat::Gif, "image/gif"),
        ("webp", image::ImageFormat::WebP, "image/webp"),
        ("bmp", image::ImageFormat::Bmp, "image/bmp"),
    ] {
        let key = format!("covers/resizable.{suffix}");
        let original = encoded_cover(format);
        fixture
            .store
            .put(&key, bytes::Bytes::copy_from_slice(&original))
            .await
            .unwrap();
        sqlx::query("UPDATE albums SET cover_key = ? WHERE id = ?")
            .bind(&key)
            .bind(fixture.album_id)
            .execute(fixture.index.pool())
            .await
            .unwrap();

        let resized = fixture
            .get(&fixture.uri(&format!("/rest/getCoverArt?id={key}&size=10")))
            .await;
        assert_eq!(resized.status(), StatusCode::OK);
        assert_eq!(resized.headers()[header::CONTENT_TYPE], mime);
        let resized = resized.into_body().collect().await.unwrap().to_bytes();
        let decoded = image::load_from_memory_with_format(&resized, format).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (10, 5));

        let unchanged = fixture
            .get(&fixture.uri(&format!("/rest/getCoverArt?id={key}")))
            .await;
        assert_eq!(unchanged.headers()[header::CONTENT_TYPE], mime);
        let unchanged = unchanged.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&unchanged[..], original.as_slice());
    }
}

#[tokio::test]
async fn cover_art_size_rejects_zero_and_excessive_dimensions() {
    let fixture = Fixture::new().await;
    for size in [0, 2049] {
        let body = json_body(
            fixture
                .get(&fixture.uri(&format!(
                    "/rest/getCoverArt?id=covers%2Ftest.jpg&size={size}&f=json"
                )))
                .await,
        )
        .await;
        assert_eq!(body["subsonic-response"]["error"]["code"], 10);
    }
}

#[tokio::test]
async fn cover_art_output_limit_returns_protocol_error() {
    let fixture = Fixture::new().await;
    let key = "covers/large-output.bmp";
    let image = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
        1700,
        1700,
        image::Rgb([20, 100, 200]),
    ));
    let mut original = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut original, image::ImageFormat::Bmp)
        .unwrap();
    fixture
        .store
        .put(key, Bytes::from(original.into_inner()))
        .await
        .unwrap();
    sqlx::query("UPDATE albums SET cover_key = ? WHERE id = ?")
        .bind(key)
        .bind(fixture.album_id)
        .execute(fixture.index.pool())
        .await
        .unwrap();

    let body = json_body(
        fixture
            .get(&fixture.uri(&format!("/rest/getCoverArt?id={key}&size=1700&f=json")))
            .await,
    )
    .await;
    assert_eq!(body["subsonic-response"]["status"], "failed");
    assert_eq!(body["subsonic-response"]["error"]["code"], 0);
}

struct BlockingCoverStore {
    inner: MemoryStore,
    active: AtomicUsize,
    entered: AtomicUsize,
    max_active: AtomicUsize,
    release: tokio::sync::Notify,
}

impl BlockingCoverStore {
    fn new() -> Self {
        Self {
            inner: MemoryStore::new(),
            active: AtomicUsize::new(0),
            entered: AtomicUsize::new(0),
            max_active: AtomicUsize::new(0),
            release: tokio::sync::Notify::new(),
        }
    }
}

#[async_trait]
impl ObjectStore for BlockingCoverStore {
    async fn list(&self, prefix: &str, token: Option<String>) -> StoreResult<ListPage> {
        self.inner.list(prefix, token).await
    }

    async fn get(&self, key: &str) -> StoreResult<Bytes> {
        self.inner.get(key).await
    }

    async fn get_range(&self, key: &str, range: Range<u64>) -> StoreResult<Bytes> {
        if key == "covers/concurrent.png" {
            let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.entered.fetch_add(1, Ordering::SeqCst);
            self.max_active.fetch_max(active, Ordering::SeqCst);
            self.release.notified().await;
            self.active.fetch_sub(1, Ordering::SeqCst);
        }
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
async fn cover_art_resize_expensive_stage_is_limited_to_two_requests() {
    let fixture = Fixture::new().await;
    let store = Arc::new(BlockingCoverStore::new());
    store
        .put(
            "covers/concurrent.png",
            Bytes::from(encoded_cover(image::ImageFormat::Png)),
        )
        .await
        .unwrap();
    sqlx::query("UPDATE albums SET cover_key = 'covers/concurrent.png' WHERE id = ?")
        .bind(fixture.album_id)
        .execute(fixture.index.pool())
        .await
        .unwrap();
    let object_store: Arc<dyn ObjectStore> = store.clone();
    let state = AppState::new(
        fixture.index.clone(),
        object_store,
        "test-secret",
        "/missing/ffmpeg",
    );

    let mut requests = Vec::new();
    for _ in 0..3 {
        let app = yevune_server::app(state.clone());
        let uri = fixture.uri("/rest/getCoverArt?id=covers/concurrent.png&size=10");
        requests.push(tokio::spawn(async move {
            app.oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
                .await
                .unwrap()
        }));
    }
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while store.entered.load(Ordering::SeqCst) < 2 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let third_entered = tokio::time::timeout(std::time::Duration::from_millis(100), async {
        while store.entered.load(Ordering::SeqCst) < 3 {
            tokio::task::yield_now().await;
        }
    })
    .await;
    assert!(
        third_entered.is_err(),
        "第三个昂贵阶段必须等待专用 semaphore"
    );
    assert_eq!(store.entered.load(Ordering::SeqCst), 2);
    assert_eq!(store.max_active.load(Ordering::SeqCst), 2);

    store.release.notify_waiters();
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while store.entered.load(Ordering::SeqCst) < 3 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    store.release.notify_one();
    for request in requests {
        assert_eq!(request.await.unwrap().status(), StatusCode::OK);
    }
}

#[tokio::test]
async fn member_can_read_and_change_only_self() {
    let fixture = Fixture::new().await;
    let self_user = json_body(
        fixture
            .get("/rest/getUser?username=member&u=member&p=secret&v=1.16.1&c=test&f=json")
            .await,
    )
    .await;
    assert_eq!(self_user["subsonic-response"]["user"]["username"], "member");

    let changed = json_body(
        fixture
            .get("/rest/changePassword?username=member&password=newpw&u=member&p=secret&v=1.16.1&c=test&f=json")
            .await,
    )
    .await;
    assert_eq!(changed["subsonic-response"]["status"], "ok");

    let denied = json_body(
        fixture
            .get("/rest/getUser?username=admin&u=member&p=newpw&v=1.16.1&c=test&f=json")
            .await,
    )
    .await;
    assert_eq!(denied["subsonic-response"]["error"]["code"], 50);
}

#[tokio::test]
async fn empty_search_returns_library_for_offline_sync() {
    let fixture = Fixture::new().await;
    let body = json_body(
        fixture
            .get(&fixture.uri("/rest/search3?query=&f=json"))
            .await,
    )
    .await;
    assert_eq!(
        body["subsonic-response"]["searchResult3"]["song"][0]["id"],
        format!("tr-{}", fixture.track_id)
    );
}

#[tokio::test]
async fn create_playlist_with_playlist_id_replaces_existing_tracks() {
    let fixture = Fixture::new().await;
    sqlx::query("UPDATE playlists SET changed_at = '2000-01-01 00:00:00' WHERE id = ?")
        .bind(fixture.playlist_id)
        .execute(fixture.index.pool())
        .await
        .unwrap();
    let body = json_body(
        fixture
            .get(&fixture.uri(&format!(
                "/rest/createPlaylist?playlistId=pl-{}&f=json",
                fixture.playlist_id
            )))
            .await,
    )
    .await;
    assert_eq!(
        body["subsonic-response"]["playlist"]["entry"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    assert_ne!(
        body["subsonic-response"]["playlist"]["changed"], "2000-01-01T00:00:00Z",
        "替换歌单曲目必须刷新 changed"
    );
    let changed = assert_rfc3339_utc(&body["subsonic-response"]["playlist"]["changed"]);
    assert!(changed > chrono::DateTime::parse_from_rfc3339("2000-01-01T00:00:00Z").unwrap());
}

#[tokio::test]
async fn album_list_frequent_uses_current_users_play_counts() {
    let fixture = Fixture::new().await;
    let other_artist = fixture
        .index
        .media()
        .upsert_artist("Another")
        .await
        .unwrap();
    fixture
        .index
        .media()
        .upsert_album("A Quiet Album", Some(other_artist), None, None)
        .await
        .unwrap();
    fixture
        .index
        .annotations()
        .scrobble(fixture.admin_id, "track", fixture.track_id)
        .await
        .unwrap();
    let body = json_body(
        fixture
            .get(&fixture.uri("/rest/getAlbumList2?type=frequent&f=json"))
            .await,
    )
    .await;
    assert_eq!(
        body["subsonic-response"]["albumList2"]["album"][0]["id"],
        format!("al-{}", fixture.album_id)
    );
}

#[tokio::test]
async fn opaque_ids_round_trip_and_set_rating_distinguishes_entity_types() {
    let fixture = Fixture::new().await;
    for (kind, prefix, id, rating) in [
        ("track", "tr", fixture.track_id, 3),
        ("album", "al", fixture.album_id, 4),
        ("artist", "ar", fixture.artist_id, 5),
    ] {
        let body = json_body(
            fixture
                .get(&fixture.uri(&format!(
                    "/rest/setRating?id={prefix}-{id}&rating={rating}&f=json"
                )))
                .await,
        )
        .await;
        assert_eq!(body["subsonic-response"]["status"], "ok");
        assert_eq!(
            fixture
                .index
                .annotations()
                .get(fixture.admin_id, kind, id)
                .await
                .unwrap()
                .unwrap()
                .rating,
            Some(rating)
        );
    }

    let song = json_body(
        fixture
            .get(&fixture.uri(&format!("/rest/getSong?id=tr-{}&f=json", fixture.track_id)))
            .await,
    )
    .await;
    assert_eq!(
        song["subsonic-response"]["song"]["id"],
        format!("tr-{}", fixture.track_id)
    );
}

#[tokio::test]
async fn song_content_type_uses_standard_mime_mapping() {
    let fixture = Fixture::new().await;
    let id = fixture
        .index
        .media()
        .upsert_track(&NewTrack {
            title: "MP3 Song".into(),
            codec: Some("mp3".into()),
            object_key: "library/test.mp3".into(),
            ..NewTrack::default()
        })
        .await
        .unwrap();
    let body = json_body(
        fixture
            .get(&fixture.uri(&format!("/rest/getSong?id=tr-{id}&f=json")))
            .await,
    )
    .await;
    assert_eq!(
        body["subsonic-response"]["song"]["contentType"],
        "audio/mpeg"
    );
}

#[tokio::test]
async fn search_treats_punctuation_as_literal_text() {
    let fixture = Fixture::new().await;
    let id = fixture
        .index
        .media()
        .upsert_track(&NewTrack {
            title: "AC/DC Song".into(),
            object_key: "library/acdc.flac".into(),
            ..NewTrack::default()
        })
        .await
        .unwrap();
    let body = json_body(
        fixture
            .get(&fixture.uri("/rest/search3?query=AC%2FDC&f=json"))
            .await,
    )
    .await;
    assert_eq!(
        body["subsonic-response"]["searchResult3"]["song"][0]["id"],
        format!("tr-{id}")
    );
}

#[tokio::test]
async fn non_empty_search_applies_each_entity_offset_in_database() {
    let fixture = Fixture::new().await;
    sqlx::query(
        "WITH RECURSIVE nums(n) AS (SELECT 0 UNION ALL SELECT n + 1 FROM nums WHERE n < 1500) \
         INSERT INTO artists(name) SELECT printf('Needle Artist %04d', n) FROM nums",
    )
    .execute(fixture.index.pool())
    .await
    .unwrap();

    let body = json_body(
        fixture
            .get(&fixture.uri(
                "/rest/search3?query=Needle&artistOffset=1500&artistCount=1&albumCount=0&songCount=0&f=json",
            ))
            .await,
    )
    .await;
    assert_eq!(
        body["subsonic-response"]["searchResult3"]["artist"]
            .as_array()
            .unwrap()
            .len(),
        1,
        "非空搜索必须在数据库侧应用各实体 offset，不能被固定的预取上限截断"
    );
}

#[tokio::test]
async fn scrobble_preserves_client_play_time() {
    let fixture = Fixture::new().await;
    let body = json_body(
        fixture
            .get(&fixture.uri(&format!(
                "/rest/scrobble?id=tr-{}&time=1000000000000&submission=true&f=json",
                fixture.track_id
            )))
            .await,
    )
    .await;
    assert_eq!(body["subsonic-response"]["status"], "ok");
    let annotation = fixture
        .index
        .annotations()
        .get(fixture.admin_id, "track", fixture.track_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        annotation.last_played.as_deref(),
        Some("2001-09-09 01:46:40")
    );
}

#[tokio::test]
async fn playlist_create_rolls_back_when_track_update_fails() {
    let fixture = Fixture::new().await;
    let before = fixture
        .index
        .playlists()
        .list_playlists(fixture.admin_id)
        .await
        .unwrap()
        .len();
    let body = json_body(
        fixture
            .get(&fixture.uri("/rest/createPlaylist?name=Broken&songId=999999&f=json"))
            .await,
    )
    .await;
    assert_eq!(body["subsonic-response"]["status"], "failed");
    let after = fixture
        .index
        .playlists()
        .list_playlists(fixture.admin_id)
        .await
        .unwrap()
        .len();
    assert_eq!(after, before);
}

#[tokio::test]
async fn malformed_playlist_id_cannot_fall_through_to_create() {
    let fixture = Fixture::new().await;
    let body = json_body(
        fixture
            .get(&fixture.uri("/rest/createPlaylist?playlistId=garbage&name=Nope&f=json"))
            .await,
    )
    .await;
    assert_eq!(body["subsonic-response"]["error"]["code"], 10);
}

#[tokio::test]
async fn unsupported_user_permissions_fail_instead_of_claiming_success() {
    let fixture = Fixture::new().await;
    let body = json_body(
        fixture
            .get(&fixture.uri(
                "/rest/createUser?username=locked&password=pw&email=x%40example.test&streamRole=false&f=json",
            ))
            .await,
    )
    .await;
    assert_eq!(body["subsonic-response"]["status"], "failed");
    assert!(fixture
        .index
        .users()
        .get_user_by_name("locked")
        .await
        .unwrap()
        .is_none());
}
