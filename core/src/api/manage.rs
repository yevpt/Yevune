//! 管理员曲库写操作。

use std::collections::HashSet;
use std::io::Read;
use std::path::Path;

use contract::{TagField, Track};
use serde::Deserialize;

use crate::auth::AuthenticatedSession;
use crate::error::{CoreError, Result};
use crate::http::HttpClient;

/// 标签覆盖层的可编辑字段；`None` 表示保持原值。
#[derive(Clone, uniffi::Record)]
pub struct TagUpdate {
    pub title: Option<String>,
    pub album: Option<String>,
    pub artist: Option<String>,
    pub genre: Option<String>,
    pub year: Option<u32>,
    pub track: Option<u32>,
    pub disc_number: Option<u32>,
    pub clear_fields: Vec<TagField>,
}

/// 上传时传给服务端的曲库目标信息。
#[derive(Clone, uniffi::Record)]
pub struct UploadMetadata {
    /// Garage 曲库对象键；必须以 `library/` 开头。
    pub library_key: String,
}

/// 上传过程的进度通知；回调只携带计数，不传递音频字节。
#[uniffi::export(callback_interface)]
pub trait UploadProgress: Send + Sync {
    /// 已读取并发往 HTTP 请求体的字节数与文件总字节数。
    fn on_progress(&self, sent_bytes: u64, total_bytes: u64);
}

pub(crate) async fn upload_track(
    http: &HttpClient,
    auth: &AuthenticatedSession,
    local_path: String,
    metadata: UploadMetadata,
    progress: Box<dyn UploadProgress>,
) -> Result<Track> {
    http.upload_track(auth, local_path, metadata.library_key, progress)
        .await
}

pub(crate) async fn update_tags(
    http: &HttpClient,
    auth: &AuthenticatedSession,
    id: String,
    update: TagUpdate,
) -> Result<()> {
    let parameters = tag_parameters(id, update)?;
    http.get_empty_with_params(auth, "ext/updateTags", &parameters)
        .await
}

fn tag_parameters(id: String, update: TagUpdate) -> Result<Vec<(String, String)>> {
    let mut parameters = vec![("id".to_owned(), id)];
    let mut set_fields = HashSet::new();
    for (field, name, value) in [
        (None, "title", update.title),
        (Some(TagField::Album), "album", update.album),
        (Some(TagField::Artist), "artist", update.artist),
        (Some(TagField::Genre), "genre", update.genre),
    ] {
        if let Some(value) = value {
            if let Some(field) = field {
                set_fields.insert(field);
            }
            parameters.push((name.to_owned(), value));
        }
    }
    for (field, name, value, maximum) in [
        (TagField::Year, "year", update.year, 9_999),
        (TagField::Track, "track", update.track, 999),
        (TagField::DiscNumber, "discNumber", update.disc_number, 999),
    ] {
        if let Some(value) = value {
            if !(1..=maximum).contains(&value) {
                return Err(CoreError::InvalidRequest {
                    message: format!("{name} 超出允许范围"),
                });
            }
            set_fields.insert(field);
            parameters.push((name.to_owned(), value.to_string()));
        }
    }
    let mut cleared = HashSet::new();
    for field in update.clear_fields {
        if !cleared.insert(field) || set_fields.contains(&field) {
            return Err(CoreError::InvalidRequest {
                message: "同一标签字段不能同时设置和清空".to_owned(),
            });
        }
        parameters.push(("clear".to_owned(), tag_field_name(field).to_owned()));
    }
    if parameters.len() == 1 {
        return Err(CoreError::InvalidRequest {
            message: "至少需要修改一个标签字段".to_owned(),
        });
    }
    Ok(parameters)
}

fn tag_field_name(field: TagField) -> &'static str {
    match field {
        TagField::Album => "album",
        TagField::Artist => "artist",
        TagField::Genre => "genre",
        TagField::Year => "year",
        TagField::Track => "track",
        TagField::DiscNumber => "discNumber",
    }
}

pub(crate) async fn delete_track(
    http: &HttpClient,
    auth: &AuthenticatedSession,
    id: String,
) -> Result<()> {
    http.get_empty_with_params(auth, "ext/deleteTrack", &[("id".to_owned(), id)])
        .await
}

pub(crate) async fn move_track(
    http: &HttpClient,
    auth: &AuthenticatedSession,
    id: String,
    key: String,
) -> Result<()> {
    http.get_empty_with_params(
        auth,
        "ext/moveTrack",
        &[("id".to_owned(), id), ("key".to_owned(), key)],
    )
    .await
}

pub(crate) fn blocking_upload(
    client: &reqwest::blocking::Client,
    url: reqwest::Url,
    local_path: String,
    key: String,
    progress: Box<dyn UploadProgress>,
) -> Result<Track> {
    let path = Path::new(&local_path);
    let file = std::fs::File::open(path).map_err(file_error)?;
    let total_bytes = file.metadata().map_err(file_error)?.len();
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("track")
        .to_owned();
    let reader = ProgressReader {
        file,
        total_bytes,
        sent_bytes: 0,
        progress,
    };
    let part = reqwest::blocking::multipart::Part::reader_with_length(reader, total_bytes)
        .file_name(filename);
    let form = reqwest::blocking::multipart::Form::new()
        .text("key", key)
        .part("file", part);
    let response = client
        .post(url)
        .multipart(form)
        .send()
        .map_err(network_error)?
        .error_for_status()
        .map_err(network_error)?;
    let envelope: UploadEnvelope = response.json().map_err(network_error)?;
    if envelope.response.status == "ok" {
        return envelope.response.track.ok_or(CoreError::InvalidResponse {
            message: "上传响应缺少 track".to_owned(),
        });
    }
    let error = envelope.response.error.unwrap_or(ServerError {
        code: 0,
        message: "服务端返回未知失败".to_owned(),
    });
    Err(CoreError::Server {
        code: error.code,
        message: error.message,
    })
}

struct ProgressReader {
    file: std::fs::File,
    total_bytes: u64,
    sent_bytes: u64,
    progress: Box<dyn UploadProgress>,
}

impl Read for ProgressReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let read = self.file.read(buffer)?;
        if read != 0 {
            self.sent_bytes += read as u64;
            self.progress.on_progress(self.sent_bytes, self.total_bytes);
        }
        Ok(read)
    }
}

#[derive(Deserialize)]
struct UploadEnvelope {
    #[serde(rename = "subsonic-response")]
    response: UploadResponse,
}

#[derive(Deserialize)]
struct UploadResponse {
    status: String,
    error: Option<ServerError>,
    track: Option<Track>,
}

#[derive(Deserialize)]
struct ServerError {
    code: u32,
    message: String,
}

fn file_error(error: std::io::Error) -> CoreError {
    CoreError::File {
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
    use contract::TagField;

    use super::{tag_parameters, TagUpdate};

    fn empty_update() -> TagUpdate {
        TagUpdate {
            title: None,
            album: None,
            artist: None,
            genre: None,
            year: None,
            track: None,
            disc_number: None,
            clear_fields: Vec::new(),
        }
    }

    #[test]
    fn rejects_setting_and_clearing_the_same_field() {
        let update = TagUpdate {
            genre: Some("Jazz".into()),
            clear_fields: vec![TagField::Genre],
            ..empty_update()
        };
        assert!(tag_parameters("tr-1".into(), update).is_err());
    }

    #[test]
    fn rejects_duplicate_clear_fields() {
        let update = TagUpdate {
            clear_fields: vec![TagField::Year, TagField::Year],
            ..empty_update()
        };
        assert!(tag_parameters("tr-1".into(), update).is_err());
    }

    #[test]
    fn rejects_year_below_supported_range() {
        let update = TagUpdate {
            year: Some(0),
            ..empty_update()
        };
        assert!(tag_parameters("tr-1".into(), update).is_err());
    }

    #[test]
    fn rejects_year_above_supported_range() {
        let update = TagUpdate {
            year: Some(10_000),
            ..empty_update()
        };
        assert!(tag_parameters("tr-1".into(), update).is_err());
    }

    #[test]
    fn rejects_track_below_supported_range() {
        let update = TagUpdate {
            track: Some(0),
            ..empty_update()
        };
        assert!(tag_parameters("tr-1".into(), update).is_err());
    }

    #[test]
    fn rejects_disc_number_above_supported_range() {
        let update = TagUpdate {
            disc_number: Some(1_000),
            ..empty_update()
        };
        assert!(tag_parameters("tr-1".into(), update).is_err());
    }
}
