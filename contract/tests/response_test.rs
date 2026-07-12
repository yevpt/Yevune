//! subsonic-response 信封的形状与 round-trip 测试。

use contract::response::{SubsonicError, SubsonicResponse};
use serde::{Deserialize, Serialize};

/// 一个带字段的载荷，用于验证 payload 被扁平化并入 subsonic-response 对象。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArtistsPayload {
    artist_count: u32,
}

#[test]
fn ok_信封形状对齐_opensubsonic() {
    let resp = SubsonicResponse::ok("yevune-server", "0.1.0", ArtistsPayload { artist_count: 3 });
    let json = serde_json::to_value(&resp).unwrap();

    let body = &json["subsonic-response"];
    assert!(body.is_object(), "顶层应有 subsonic-response 对象：{json}");
    assert_eq!(body["status"], "ok");
    assert_eq!(body["type"], "yevune-server");
    assert_eq!(body["serverVersion"], "0.1.0");
    assert_eq!(body["version"], "1.16.1");
    assert_eq!(body["openSubsonic"], true);
    // payload 应被扁平化并入 body
    assert_eq!(body["artistCount"], 3);
    assert!(body.get("error").is_none(), "成功响应不应含 error");
}

#[test]
fn failed_信封含_error() {
    let resp = SubsonicResponse::<ArtistsPayload>::failed(
        "yevune-server",
        "0.1.0",
        SubsonicError {
            code: 40,
            message: "Wrong username or password.".into(),
        },
    );
    let json = serde_json::to_value(&resp).unwrap();

    let body = &json["subsonic-response"];
    assert_eq!(body["status"], "failed");
    assert_eq!(body["error"]["code"], 40);
    assert_eq!(body["error"]["message"], "Wrong username or password.");
}

#[test]
fn 信封往返() {
    let resp = SubsonicResponse::ok("yevune-server", "0.1.0", ArtistsPayload { artist_count: 7 });
    let text = serde_json::to_string(&resp).unwrap();
    let back: SubsonicResponse<ArtistsPayload> = serde_json::from_str(&text).unwrap();
    assert_eq!(back, resp);
}
