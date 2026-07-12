//! OpenSubsonic JSON/XML 统一响应与协议错误映射。

use axum::http::{header, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use contract::{Album, Artist, Playlist, Track, User};
use serde_json::{Map, Value};

use crate::auth::AuthError;

pub const SERVER_TYPE: &str = "yevune-server";
pub const API_VERSION: &str = contract::response::OPEN_SUBSONIC_API_VERSION;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Xml,
    Json,
}

impl Format {
    pub fn from_uri(uri: &Uri) -> Self {
        let is_json = uri.query().is_some_and(|query| {
            query
                .split('&')
                .any(|part| part.eq_ignore_ascii_case("f=json"))
        });
        if is_json {
            Self::Json
        } else {
            Self::Xml
        }
    }
}

pub fn ok(format: Format, data: Value) -> Response {
    envelope(format, "ok", Some(data), None)
}

pub fn empty(format: Format) -> Response {
    ok(format, Value::Object(Map::new()))
}

pub fn error(format: Format, code: u32, message: &str) -> Response {
    envelope(
        format,
        "failed",
        None,
        Some(serde_json::json!({"code": code, "message": message})),
    )
}

fn envelope(format: Format, status: &str, data: Option<Value>, error: Option<Value>) -> Response {
    let mut body = Map::new();
    body.insert("status".into(), status.into());
    body.insert("version".into(), API_VERSION.into());
    body.insert("type".into(), SERVER_TYPE.into());
    body.insert("serverVersion".into(), env!("CARGO_PKG_VERSION").into());
    body.insert("openSubsonic".into(), true.into());
    if let Some(Value::Object(data)) = data {
        body.extend(data);
    }
    if let Some(error) = error {
        body.insert("error".into(), error);
    }

    match format {
        Format::Json => axum::Json(serde_json::json!({"subsonic-response": body})).into_response(),
        Format::Xml => {
            let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
            xml.push_str("<subsonic-response xmlns=\"http://subsonic.org/restapi\"");
            let attributes = ["status", "version", "type", "serverVersion", "openSubsonic"];
            for name in attributes {
                if let Some(value) = body.remove(name) {
                    xml.push(' ');
                    xml.push_str(name);
                    xml.push_str("=\"");
                    xml.push_str(&escape(&scalar(&value)));
                    xml.push('"');
                }
            }
            if body.is_empty() {
                xml.push_str("/>");
            } else {
                xml.push('>');
                for (name, value) in body {
                    write_xml(&mut xml, &name, &value);
                }
                xml.push_str("</subsonic-response>");
            }
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/xml; charset=utf-8")],
                xml,
            )
                .into_response()
        }
    }
}

fn write_xml(out: &mut String, name: &str, value: &Value) {
    match value {
        Value::Array(items) => {
            for item in items {
                write_xml(out, name, item);
            }
        }
        Value::Object(object) => {
            out.push('<');
            out.push_str(name);
            for (key, value) in object {
                if key == "value" || value.is_array() || value.is_object() || value.is_null() {
                    continue;
                }
                out.push(' ');
                out.push_str(key);
                out.push_str("=\"");
                out.push_str(&escape(&scalar(value)));
                out.push('"');
            }
            let has_children = object
                .iter()
                .any(|(key, value)| value.is_array() || value.is_object() || key == "value");
            if !has_children {
                out.push_str("/>");
                return;
            }
            out.push('>');
            if let Some(value) = object.get("value") {
                out.push_str(&escape(&scalar(value)));
            }
            for (key, value) in object {
                if key != "value" && (value.is_array() || value.is_object()) {
                    write_xml(out, key, value);
                }
            }
            out.push_str("</");
            out.push_str(name);
            out.push('>');
        }
        _ => {
            out.push('<');
            out.push_str(name);
            out.push('>');
            out.push_str(&escape(&scalar(value)));
            out.push_str("</");
            out.push_str(name);
            out.push('>');
        }
    }
}

fn scalar(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub fn auth_error(format: Format, error_value: AuthError) -> Response {
    let code = error_value.subsonic_code();
    let message = match code {
        10 => "Required parameter is missing or malformed",
        40 => "Wrong username or password",
        50 => "User is not authorized for the given operation",
        _ => "Internal server error",
    };
    error(format, code, message)
}

pub fn parameter_error(format: Format, message: &str) -> Response {
    error(format, 10, message)
}

pub fn not_found(format: Format) -> Response {
    error(format, 70, "The requested data was not found")
}

pub fn internal(format: Format) -> Response {
    error(format, 0, "Internal server error")
}

pub fn query_i64_values(uri: &Uri, name: &str) -> Result<Vec<i64>, ()> {
    let mut values = Vec::new();
    for pair in uri.query().unwrap_or_default().split('&') {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        if key == name {
            values.push(value.parse().map_err(|_| ())?);
        }
    }
    Ok(values)
}

pub fn query_entity_ids(uri: &Uri, name: &str, kind: &str) -> Result<Vec<i64>, ()> {
    let mut values = Vec::new();
    for pair in uri.query().unwrap_or_default().split('&') {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        if key == name {
            values.push(parse_entity_id(value, kind).ok_or(())?);
        }
    }
    Ok(values)
}

pub fn parse_entity_id(value: &str, kind: &str) -> Option<i64> {
    let prefix = match kind {
        "track" => "tr-",
        "album" => "al-",
        "artist" => "ar-",
        "playlist" => "pl-",
        "folder" => "pf-",
        "user" => "us-",
        "role" => "ro-",
        "rule" => "ru-",
        _ => return None,
    };
    if let Some(raw) = value.strip_prefix(prefix) {
        return raw.parse().ok();
    }
    (kind == "track").then(|| value.parse().ok()).flatten()
}

pub fn parse_ratable_id(value: &str) -> Option<(&'static str, i64)> {
    for (kind, prefix) in [("track", "tr-"), ("album", "al-"), ("artist", "ar-")] {
        if let Some(raw) = value.strip_prefix(prefix) {
            return Some((kind, raw.parse().ok()?));
        }
    }
    Some(("track", value.parse().ok()?))
}

pub(crate) fn opaque_id(kind: &str, value: &str) -> String {
    let prefix = match kind {
        "track" => "tr-",
        "album" => "al-",
        "artist" => "ar-",
        "playlist" => "pl-",
        "folder" => "pf-",
        "user" => "us-",
        "role" => "ro-",
        "rule" => "ru-",
        _ => "",
    };
    format!("{prefix}{value}")
}

pub fn artist_value(artist: &Artist) -> Value {
    let mut value = serde_json::to_value(artist).expect("Artist DTO 必须可序列化");
    remove_nulls(&mut value);
    value
        .as_object_mut()
        .expect("artist 是对象")
        .insert("id".into(), opaque_id("artist", &artist.id).into());
    value
}

pub fn album_value(album: &Album) -> Value {
    let mut value = serde_json::to_value(album).expect("Album DTO 必须可序列化");
    remove_nulls(&mut value);
    if let Value::Object(object) = &mut value {
        object.insert("id".into(), opaque_id("album", &album.id).into());
        if let Some(artist_id) = &album.artist_id {
            object.insert("artistId".into(), opaque_id("artist", artist_id).into());
        }
        object.insert("album".into(), album.name.clone().into());
        object.insert("title".into(), album.name.clone().into());
        object.insert("isDir".into(), true.into());
    }
    value
}

pub fn track_value(track: &Track) -> Value {
    let mut value = serde_json::to_value(track).expect("Track DTO 必须可序列化");
    remove_nulls(&mut value);
    if let Value::Object(object) = &mut value {
        object.insert("id".into(), opaque_id("track", &track.id).into());
        if let Some(album_id) = &track.album_id {
            object.insert("albumId".into(), opaque_id("album", album_id).into());
        }
        if let Some(artist_id) = &track.artist_id {
            object.insert("artistId".into(), opaque_id("artist", artist_id).into());
        }
        object.insert("isDir".into(), false.into());
        object.insert("type".into(), "music".into());
        if let Some(suffix) = &track.suffix {
            object.insert("contentType".into(), mime_type(suffix).into());
        }
    }
    value
}

pub fn mime_type(codec: &str) -> &'static str {
    match codec.to_ascii_lowercase().as_str() {
        "mp3" => "audio/mpeg",
        "aac" => "audio/aac",
        "m4a" | "mp4" | "alac" => "audio/mp4",
        "opus" | "ogg" | "oga" => "audio/ogg",
        "flac" => "audio/flac",
        "wav" => "audio/wav",
        "wma" => "audio/x-ms-wma",
        "ape" => "audio/ape",
        _ => "application/octet-stream",
    }
}

pub fn playlist_value(playlist: &Playlist, owner: &str) -> Value {
    serde_json::json!({
        "id": opaque_id("playlist", &playlist.id),
        "name": playlist.name,
        "comment": playlist.comment,
        "owner": owner,
        "public": false,
        "songCount": playlist.song_count,
        "duration": playlist.duration,
        "created": playlist.created,
        "changed": playlist.changed,
    })
}

pub fn user_value(user: &User) -> Value {
    serde_json::json!({
        "username": user.name,
        "email": user.email,
        "scrobblingEnabled": true,
        "adminRole": user.admin,
        "settingsRole": true,
        "downloadRole": true,
        "uploadRole": user.admin,
        "playlistRole": true,
        "coverArtRole": true,
        "commentRole": true,
        "podcastRole": false,
        "streamRole": true,
        "jukeboxRole": false,
        "shareRole": false,
        "videoConversionRole": false,
    })
}

fn remove_nulls(value: &mut Value) {
    match value {
        Value::Object(object) => {
            object.retain(|_, value| !value.is_null());
            for value in object.values_mut() {
                remove_nulls(value);
            }
        }
        Value::Array(values) => {
            for value in values {
                remove_nulls(value);
            }
        }
        _ => {}
    }
}
