//! 管理员曲库写操作。

use std::io::Read;
use std::path::Path;

use contract::Track;
use serde::Deserialize;

use crate::auth::AuthenticatedSession;
use crate::error::{CoreError, Result};
use crate::http::HttpClient;

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
        message: error.to_string(),
    }
}
