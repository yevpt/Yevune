//! 媒体 URL 与封面替换；媒体字节由系统播放器/流式 multipart 直接传输。

use std::path::Path;

use crate::auth::AuthenticatedSession;
use crate::error::{CoreError, Result};

pub(crate) fn cover_art_url(
    auth: &AuthenticatedSession,
    id: String,
    size: Option<u32>,
) -> Result<String> {
    signed_url(
        auth,
        "getCoverArt",
        id,
        size.map(|value| ("size", value.to_string())),
    )
}

pub(crate) fn stream_url(auth: &AuthenticatedSession, id: String) -> Result<String> {
    signed_url(auth, "stream", id, None)
}

fn signed_url(
    auth: &AuthenticatedSession,
    endpoint: &str,
    id: String,
    extra: Option<(&str, String)>,
) -> Result<String> {
    let mut url = auth.config.endpoint(endpoint)?;
    let mut query = url.query_pairs_mut();
    query.extend_pairs(auth.query_pairs());
    query.append_pair("id", &id);
    if let Some((key, value)) = extra {
        query.append_pair(key, &value);
    }
    drop(query);
    Ok(url.into())
}

pub(crate) async fn set_cover_art(
    auth: &AuthenticatedSession,
    album_id: String,
    local_path: String,
) -> Result<()> {
    let mut url = auth.config.endpoint("ext/setCoverArt")?;
    url.query_pairs_mut().extend_pairs(auth.query_pairs());
    tokio::task::spawn_blocking(move || {
        let path = Path::new(&local_path);
        let file = std::fs::File::open(path).map_err(file_error)?;
        let length = file.metadata().map_err(file_error)?.len();
        let part = reqwest::blocking::multipart::Part::reader_with_length(file, length).file_name(
            path.file_name()
                .and_then(|v| v.to_str())
                .unwrap_or("cover")
                .to_owned(),
        );
        let response = reqwest::blocking::Client::new()
            .post(url)
            .multipart(
                reqwest::blocking::multipart::Form::new()
                    .text("id", album_id)
                    .part("file", part),
            )
            .send()
            .map_err(network_error)?
            .error_for_status()
            .map_err(network_error)?;
        let body: serde_json::Value = response.json().map_err(network_error)?;
        if body["subsonic-response"]["status"] == "ok" {
            Ok(())
        } else {
            Err(CoreError::Server {
                code: body["subsonic-response"]["error"]["code"]
                    .as_u64()
                    .unwrap_or(0) as u32,
                message: body["subsonic-response"]["error"]["message"]
                    .as_str()
                    .unwrap_or("替换封面失败")
                    .to_owned(),
            })
        }
    })
    .await
    .map_err(|error| CoreError::Network {
        message: error.to_string(),
    })?
}

fn file_error(error: std::io::Error) -> CoreError {
    CoreError::InvalidRequest {
        message: error.to_string(),
    }
}
fn network_error(error: reqwest::Error) -> CoreError {
    CoreError::Network {
        message: error.without_url().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ServerConfig;
    fn auth() -> AuthenticatedSession {
        AuthenticatedSession {
            config: ServerConfig::parse("http://music.local").unwrap(),
            user: "u".into(),
            password: "p".into(),
        }
    }
    #[test]
    fn media_urls_are_authenticated() {
        let cover = cover_art_url(&auth(), "covers/a b.jpg".into(), Some(512)).unwrap();
        assert!(cover.contains("getCoverArt"));
        assert!(cover.contains("size=512"));
        assert!(cover.contains("p=p"));
        let stream = stream_url(&auth(), "tr-7".into()).unwrap();
        assert!(stream.contains("stream"));
        assert!(stream.contains("id=tr-7"));
    }
}
