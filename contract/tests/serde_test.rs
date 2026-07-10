//! serde round-trip 测试：验证类型可无损往返，且序列化字段名对齐 OpenSubsonic（camelCase）。

use contract::{
    AccessRule, Album, Artist, Genre, Playlist, PlaylistFolder, Principal, PrincipalType, Role,
    ScopeType, StreamRequest, Track, User,
};
use serde_json::Value;

/// 序列化为 JSON，断言其为对象且包含全部期望键，再反序列化断言与原值相等。
fn assert_roundtrip<T>(value: &T, expected_keys: &[&str]) -> Value
where
    T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let json = serde_json::to_value(value).expect("序列化失败");
    let obj = json.as_object().expect("应序列化为 JSON 对象");
    for key in expected_keys {
        assert!(
            obj.contains_key(*key),
            "缺少 OpenSubsonic 字段名 `{key}`，实际字段：{:?}",
            obj.keys().collect::<Vec<_>>()
        );
    }
    let back: T = serde_json::from_value(json.clone()).expect("反序列化失败");
    assert_eq!(&back, value, "round-trip 后不相等");
    json
}

#[test]
fn genre_往返且字段名对齐() {
    let g = Genre {
        value: "Rock".into(),
        song_count: 12,
        album_count: 3,
    };
    assert_roundtrip(&g, &["value", "songCount", "albumCount"]);
}

#[test]
fn artist_往返且字段名对齐() {
    let a = Artist {
        id: "ar-1".into(),
        name: "周杰伦".into(),
        sort_name: Some("Zhou Jielun".into()),
        cover_art: Some("cover-ar-1".into()),
        music_brainz_id: Some("mbid-1".into()),
        album_count: 5,
    };
    assert_roundtrip(
        &a,
        &[
            "id",
            "name",
            "sortName",
            "coverArt",
            "musicBrainzId",
            "albumCount",
        ],
    );
}

#[test]
fn album_往返且字段名对齐() {
    let a = Album {
        id: "al-1".into(),
        name: "叶惠美".into(),
        artist: Some("周杰伦".into()),
        artist_id: Some("ar-1".into()),
        cover_art: Some("cover-al-1".into()),
        song_count: 10,
        duration: 2600,
        year: Some(2003),
        genre: Some("Mandopop".into()),
        created: Some("2026-07-10T00:00:00Z".into()),
    };
    assert_roundtrip(
        &a,
        &[
            "id",
            "name",
            "artist",
            "artistId",
            "coverArt",
            "songCount",
            "duration",
            "year",
            "genre",
            "created",
        ],
    );
}

#[test]
fn track_往返且字段名对齐() {
    let t = Track {
        id: "tr-1".into(),
        title: "晴天".into(),
        album: Some("叶惠美".into()),
        album_id: Some("al-1".into()),
        artist: Some("周杰伦".into()),
        artist_id: Some("ar-1".into()),
        track: Some(6),
        disc_number: Some(1),
        year: Some(2003),
        genre: Some("Mandopop".into()),
        cover_art: Some("cover-al-1".into()),
        size: 41_000_000,
        content_type: Some("audio/flac".into()),
        suffix: Some("flac".into()),
        duration: 269,
        bit_rate: 1024,
        created: Some("2026-07-10T00:00:00Z".into()),
    };
    assert_roundtrip(
        &t,
        &[
            "id",
            "title",
            "albumId",
            "artistId",
            "discNumber",
            "contentType",
            "bitRate",
            "coverArt",
        ],
    );
}

#[test]
fn playlist_folder_往返且字段名对齐() {
    let f = PlaylistFolder {
        id: "fo-1".into(),
        owner_id: "us-1".into(),
        name: "中文".into(),
        parent_id: None,
        position: 0,
    };
    assert_roundtrip(&f, &["id", "ownerId", "name", "position"]);
}

#[test]
fn playlist_往返且字段名对齐() {
    let p = Playlist {
        id: "pl-1".into(),
        owner_id: "us-1".into(),
        name: "精选".into(),
        comment: Some("我的最爱".into()),
        folder_id: Some("fo-1".into()),
        position: 1,
        song_count: 20,
        duration: 5400,
    };
    assert_roundtrip(
        &p,
        &[
            "id",
            "ownerId",
            "name",
            "comment",
            "folderId",
            "position",
            "songCount",
            "duration",
        ],
    );
}

#[test]
fn user_往返且字段名对齐() {
    let u = User {
        id: "us-1".into(),
        name: "papa".into(),
        created: Some("2026-07-10T00:00:00Z".into()),
        admin: true,
        roles: vec!["admin".into(), "member".into()],
    };
    assert_roundtrip(&u, &["id", "name", "created", "admin", "roles"]);
}

#[test]
fn role_往返且字段名对齐() {
    let r = Role {
        id: "ro-1".into(),
        name: "孩子".into(),
        is_builtin: false,
    };
    assert_roundtrip(&r, &["id", "name", "isBuiltin"]);
}

#[test]
fn access_rule_往返且枚举为小写() {
    let rule = AccessRule {
        id: "ac-1".into(),
        scope_type: ScopeType::Album,
        scope_id: "al-1".into(),
        grants: vec![
            Principal {
                principal_type: PrincipalType::User,
                id: "us-1".into(),
            },
            Principal {
                principal_type: PrincipalType::Role,
                id: "ro-1".into(),
            },
        ],
    };
    let json = assert_roundtrip(&rule, &["id", "scopeType", "scopeId", "grants"]);

    // 枚举值应序列化为小写字符串
    assert_eq!(json["scopeType"], "album");
    // Principal 的类型字段名应为 `type`，值为小写
    let first = &json["grants"][0];
    assert_eq!(first["type"], "user");
    assert!(
        first.get("type").is_some(),
        "Principal 应有 `type` 字段：{first}"
    );
}

#[test]
fn stream_request_往返且字段名对齐() {
    let s = StreamRequest {
        id: "tr-1".into(),
        format: Some("opus".into()),
        max_bitrate: Some(192),
    };
    assert_roundtrip(&s, &["id", "format", "maxBitRate"]);
}
