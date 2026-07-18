//! 原始下载、按需转码流与封面二进制传输端点。

use std::io::{Seek, Write};

use axum::body::Body;
use axum::extract::{OriginalUri, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use futures::TryStreamExt;
use image::{ImageFormat, ImageReader, Limits};
use serde::Deserialize;

use crate::scanner::HEADER_READ_CAP;
use crate::storage::STREAM_CHUNK_SIZE;
use crate::transcode::{should_transcode, TranscodeTarget, TranscodeTrack};

use super::response::{self, Format};
use super::{ApiQuery, ApiUser, AppState};

const MAX_COVER_REQUESTED_SIZE: u32 = 2048;
const MAX_COVER_INPUT_BYTES: u64 = 16 * 1024 * 1024;
const MAX_COVER_INPUT_DIMENSION: u32 = 4096;
const MAX_COVER_INPUT_PIXELS: u64 = 8_000_000;
const MAX_COVER_DECODE_ALLOC: u64 = 32 * 1024 * 1024;
const MAX_COVER_OUTPUT_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StreamParams {
    id: Option<String>,
    format: Option<String>,
    max_bit_rate: Option<u32>,
    #[allow(dead_code)]
    time_offset: Option<f64>,
}

#[derive(Deserialize)]
struct IdParams {
    id: Option<String>,
}

#[derive(Deserialize)]
struct CoverParams {
    id: Option<String>,
    #[allow(dead_code)]
    size: Option<u32>,
}

pub fn router() -> Router<AppState> {
    let mut router = Router::new();
    for path in ["/rest/stream", "/rest/stream.view"] {
        router = router.route(path, get(stream));
    }
    for path in ["/rest/download", "/rest/download.view"] {
        router = router.route(path, get(download));
    }
    for path in ["/rest/getCoverArt", "/rest/getCoverArt.view"] {
        router = router.route(path, get(get_cover_art));
    }
    for path in ["/rest/getLyricsBySongId", "/rest/getLyricsBySongId.view"] {
        router = router.route(path, get(get_lyrics_by_song_id));
    }
    router
}

/// 单曲目访问控制门控结果。
enum Gate {
    /// 允许访问。
    Allow,
    /// 拒绝，附带已构造好的协议响应（受限一律以 not_found 回应，不泄漏存在性）。
    Deny(Response),
}

/// 解析访问者并判定其能否访问该曲目；受限或曲目不存在均返回 [`Gate::Deny`]。
///
/// 授权在服务端强制（设计文档 §6），客户端无法绕过流/下载端点获取受限内容。
async fn gate_track(state: &AppState, user_id: i64, track_id: i64, format: Format) -> Gate {
    let viewer = match state.viewer(user_id).await {
        Ok(viewer) => viewer,
        Err(error) => {
            tracing::error!(%error, "媒体端点解析访问者失败");
            return Gate::Deny(response::internal(format));
        }
    };
    match state
        .index
        .access_control()
        .can_access_track(&viewer, track_id)
        .await
    {
        Ok(true) => Gate::Allow,
        Ok(false) => Gate::Deny(response::not_found(format)),
        Err(error) => {
            tracing::error!(%error, "媒体端点可见性判定失败");
            Gate::Deny(response::internal(format))
        }
    }
}

async fn stream(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    ApiQuery(params): ApiQuery<StreamParams>,
    headers: HeaderMap,
    ApiUser(user): ApiUser,
) -> Response {
    let format = Format::from_uri(&uri);
    let Some(id) = params
        .id
        .as_deref()
        .and_then(|id| response::parse_entity_id(id, "track"))
    else {
        return response::parameter_error(format, "Required parameter 'id' is missing");
    };
    match gate_track(&state, user.id, id, format).await {
        Gate::Allow => {}
        Gate::Deny(response) => return response,
    }
    let source = match state.index.media().media_source(id).await {
        Ok(Some(source)) => source,
        Ok(None) => return response::not_found(format),
        Err(error) => {
            tracing::error!(%error, "stream 曲目定位失败");
            return response::internal(format);
        }
    };
    let source_codec = source.codec.as_deref().unwrap_or("flac");
    let (target_format, bitrate) = select_target(
        source_codec,
        source.bitrate.unwrap_or(0).max(0) as u32,
        params.format.as_deref(),
        params.max_bit_rate,
        &state.default_transcode_format,
        state.default_transcode_bitrate,
    );
    let target = TranscodeTarget::new(&target_format, bitrate);
    let track = TranscodeTrack::new(
        id,
        source.object_key,
        source_codec,
        source.bitrate.unwrap_or(0).max(0) as u32,
    );
    let content_type = response::mime_type(if target_format == "raw" {
        source_codec
    } else {
        &target_format
    });
    if !should_transcode(&track, &target) {
        return object_response(
            state.store.clone(),
            track.object_key,
            content_type,
            &headers,
            None,
            format,
        )
        .await;
    }
    if headers.contains_key(header::RANGE) {
        match state
            .index
            .transcode_cache()
            .get(id, &target.format, target.bitrate)
            .await
        {
            Ok(Some(cache)) => {
                if state.store.head(&cache.object_key).await.is_ok() {
                    return object_response(
                        state.store.clone(),
                        cache.object_key,
                        content_type,
                        &headers,
                        None,
                        format,
                    )
                    .await;
                }
            }
            Ok(None) => {}
            Err(error) => tracing::error!(%error, track_id = id, "stream 缓存索引查询失败"),
        }
    }
    let bytes = match state.transcoder.stream(track, target).await {
        Ok(bytes) => bytes,
        Err(error) => {
            tracing::error!(%error, track_id = id, "stream 启动失败");
            return response::internal(format);
        }
    };
    let body = Body::from_stream(bytes.map_err(std::io::Error::other));
    (StatusCode::OK, [(header::CONTENT_TYPE, content_type)], body).into_response()
}

async fn download(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    ApiQuery(params): ApiQuery<IdParams>,
    headers: HeaderMap,
    ApiUser(user): ApiUser,
) -> Response {
    let format = Format::from_uri(&uri);
    let Some(id) = params
        .id
        .as_deref()
        .and_then(|id| response::parse_entity_id(id, "track"))
    else {
        return response::parameter_error(format, "Required parameter 'id' is missing");
    };
    match gate_track(&state, user.id, id, format).await {
        Gate::Allow => {}
        Gate::Deny(response) => return response,
    }
    let source = match state.index.media().media_source(id).await {
        Ok(Some(source)) => source,
        Ok(None) => return response::not_found(format),
        Err(error) => {
            tracing::error!(%error, "download 曲目定位失败");
            return response::internal(format);
        }
    };
    let codec = source.codec.clone().unwrap_or_else(|| "bin".to_string());
    object_response(
        state.store.clone(),
        source.object_key,
        response::mime_type(&codec),
        &headers,
        Some(format!("{id}.{codec}")),
        format,
    )
    .await
}

async fn get_lyrics_by_song_id(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    ApiQuery(params): ApiQuery<IdParams>,
    ApiUser(user): ApiUser,
) -> Response {
    let format = Format::from_uri(&uri);
    let Some(id) = params
        .id
        .as_deref()
        .and_then(|id| response::parse_entity_id(id, "track"))
    else {
        return response::parameter_error(format, "Required parameter 'id' is missing");
    };
    match gate_track(&state, user.id, id, format).await {
        Gate::Allow => {}
        Gate::Deny(response) => return response,
    }
    let source = match state.index.media().media_source(id).await {
        Ok(Some(source)) => source,
        Ok(None) => return response::not_found(format),
        Err(error) => {
            tracing::error!(%error, "getLyricsBySongId 曲目定位失败");
            return response::internal(format);
        }
    };
    let size = source.size.unwrap_or(0).max(0) as u64;
    let header = match state
        .store
        .get_range(&source.object_key, 0..size.min(HEADER_READ_CAP))
        .await
    {
        Ok(header) => header,
        Err(error) => {
            tracing::error!(%error, track_id = id, "getLyricsBySongId 读取标签失败");
            return response::internal(format);
        }
    };
    let mut lyrics = match crate::scanner::tags::parse_header(header) {
        Ok(parsed) => parsed.lyrics.into_iter().collect::<Vec<_>>(),
        Err(error) => {
            tracing::warn!(%error, track_id = id, "getLyricsBySongId 无法解析标签");
            Vec::new()
        }
    };
    if let Some(lyrics) = lyrics.first_mut() {
        match state.index.media().get_track(id).await {
            Ok(Some(track)) => {
                lyrics.display_artist = track.artist;
                lyrics.display_title = Some(track.title);
            }
            Ok(None) => return response::not_found(format),
            Err(error) => {
                tracing::error!(%error, track_id = id, "getLyricsBySongId 读取曲目信息失败");
                return response::internal(format);
            }
        }
    }
    response::ok(
        format,
        serde_json::json!({"lyricsList": {"structuredLyrics": lyrics}}),
    )
}

async fn get_cover_art(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    ApiQuery(params): ApiQuery<CoverParams>,
    ApiUser(user): ApiUser,
) -> Response {
    let format = Format::from_uri(&uri);
    let Some(key) = params.id.filter(|id| !id.is_empty()) else {
        return response::parameter_error(format, "Required parameter 'id' is missing");
    };
    match state.index.media().has_cover_key(&key).await {
        Ok(true) => {}
        Ok(false) => return response::not_found(format),
        Err(error) => {
            tracing::error!(%error, "getCoverArt 封面标识查询失败");
            return response::internal(format);
        }
    }
    // 访问控制强制：封面随其归属曲目的可见性收敛，受限内容的封面不外泄。
    let viewer = match state.viewer(user.id).await {
        Ok(viewer) => viewer,
        Err(error) => {
            tracing::error!(%error, "getCoverArt 解析访问者失败");
            return response::internal(format);
        }
    };
    match state.index.media().cover_key_visible(&viewer, &key).await {
        Ok(true) => {}
        Ok(false) => return response::not_found(format),
        Err(error) => {
            tracing::error!(%error, "getCoverArt 可见性判定失败");
            return response::internal(format);
        }
    }
    let object_size = match state.store.head(&key).await {
        Ok(meta) => meta.size,
        Err(crate::storage::StorageError::NotFound(_)) => return response::not_found(format),
        Err(error) => {
            tracing::error!(%error, "getCoverArt 定位失败");
            return response::internal(format);
        }
    };
    if let Some(requested_size) = params.size {
        if requested_size == 0 || requested_size > MAX_COVER_REQUESTED_SIZE {
            return response::parameter_error(format, "size must be between 1 and 2048");
        }
        if object_size > MAX_COVER_INPUT_BYTES {
            return response::parameter_error(format, "Cover art is too large to resize");
        }
        let Some(image_format) = cover_format(&key) else {
            return response::parameter_error(format, "Cover art format cannot be resized");
        };
        let permit = match state.cover_resize_semaphore.clone().acquire_owned().await {
            Ok(permit) => permit,
            Err(error) => {
                tracing::error!(%error, "getCoverArt 缩放并发限制器已关闭");
                return response::internal(format);
            }
        };
        let input = match state.store.get_range(&key, 0..object_size).await {
            Ok(input) => input,
            Err(crate::storage::StorageError::NotFound(_)) => return response::not_found(format),
            Err(error) => {
                tracing::error!(%error, "getCoverArt 缩放输入读取失败");
                return response::internal(format);
            }
        };
        let resized = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            resize_cover(&input, image_format, requested_size)
        })
        .await;
        return match resized {
            Ok(Ok(bytes)) => (
                StatusCode::OK,
                [(header::CONTENT_TYPE, cover_type(&key))],
                bytes,
            )
                .into_response(),
            Ok(Err(error)) => {
                tracing::warn!(%error, "getCoverArt 图片解码或缩放失败");
                response::internal(format)
            }
            Err(error) => {
                tracing::error!(%error, "getCoverArt 缩放任务异常退出");
                response::internal(format)
            }
        };
    }
    let store = state.store.clone();
    let stream = futures::stream::try_unfold(
        (store, key.clone(), 0_u64, object_size),
        |(store, key, offset, object_size)| async move {
            if offset >= object_size {
                return Ok(None);
            }
            let end = (offset + STREAM_CHUNK_SIZE as u64).min(object_size);
            let bytes = store.get_range(&key, offset..end).await?;
            Ok::<_, crate::storage::StorageError>(Some((bytes, (store, key, end, object_size))))
        },
    );
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, cover_type(&key))],
        Body::from_stream(stream.map_err(std::io::Error::other)),
    )
        .into_response()
}

fn resize_cover(
    input: &[u8],
    format: ImageFormat,
    requested_size: u32,
) -> Result<Vec<u8>, image::ImageError> {
    let dimensions_reader = ImageReader::with_format(std::io::Cursor::new(input), format);
    let (width, height) = dimensions_reader.into_dimensions()?;
    if width > MAX_COVER_INPUT_DIMENSION
        || height > MAX_COVER_INPUT_DIMENSION
        || u64::from(width) * u64::from(height) > MAX_COVER_INPUT_PIXELS
    {
        return Err(image::ImageError::Limits(
            image::error::LimitError::from_kind(image::error::LimitErrorKind::DimensionError),
        ));
    }

    let mut reader = ImageReader::with_format(std::io::Cursor::new(input), format);
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_COVER_INPUT_DIMENSION);
    limits.max_image_height = Some(MAX_COVER_INPUT_DIMENSION);
    limits.max_alloc = Some(MAX_COVER_DECODE_ALLOC);
    reader.limits(limits);
    // DynamicImage 对动画 GIF 使用解码器给出的首帧，避免逐帧展开造成无界内存。
    let image = reader.decode()?;
    let resized = image.thumbnail(requested_size, requested_size);
    let mut output = BoundedWriter::new(MAX_COVER_OUTPUT_BYTES);
    resized.write_to(&mut output, format)?;
    Ok(output.into_inner())
}

struct BoundedWriter {
    inner: std::io::Cursor<Vec<u8>>,
    limit: u64,
}

impl BoundedWriter {
    fn new(limit: u64) -> Self {
        Self {
            inner: std::io::Cursor::new(Vec::new()),
            limit,
        }
    }

    fn into_inner(self) -> Vec<u8> {
        self.inner.into_inner()
    }
}

impl Write for BoundedWriter {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        let end = self
            .inner
            .position()
            .checked_add(buffer.len() as u64)
            .ok_or_else(output_limit_error)?;
        if end > self.limit {
            return Err(output_limit_error());
        }
        self.inner.write(buffer)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

impl Seek for BoundedWriter {
    fn seek(&mut self, position: std::io::SeekFrom) -> std::io::Result<u64> {
        let previous = self.inner.position();
        let next = self.inner.seek(position)?;
        if next > self.limit {
            self.inner.set_position(previous);
            return Err(output_limit_error());
        }
        Ok(next)
    }
}

fn output_limit_error() -> std::io::Error {
    std::io::Error::other("encoded cover art exceeds output limit")
}

fn cover_format(key: &str) -> Option<ImageFormat> {
    match key
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => Some(ImageFormat::Jpeg),
        "png" => Some(ImageFormat::Png),
        "gif" => Some(ImageFormat::Gif),
        "webp" => Some(ImageFormat::WebP),
        "bmp" => Some(ImageFormat::Bmp),
        _ => None,
    }
}

fn cover_type(key: &str) -> &'static str {
    match key
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        _ => "application/octet-stream",
    }
}

async fn object_response(
    store: std::sync::Arc<dyn crate::storage::ObjectStore>,
    key: String,
    content_type: &'static str,
    request_headers: &HeaderMap,
    download_name: Option<String>,
    format: Format,
) -> Response {
    let size = match store.head(&key).await {
        Ok(meta) => meta.size,
        Err(crate::storage::StorageError::NotFound(_)) => return response::not_found(format),
        Err(error) => {
            tracing::error!(%error, "媒体对象定位失败");
            return response::internal(format);
        }
    };
    let range = request_headers
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| parse_range(value, size));
    let (start, end, status) = match range {
        Some((start, end)) => (start, end, StatusCode::PARTIAL_CONTENT),
        None => (0, size, StatusCode::OK),
    };
    let stream = futures::stream::try_unfold(
        (store, key, start, end),
        |(store, key, offset, end)| async move {
            if offset >= end {
                return Ok(None);
            }
            let next = (offset + STREAM_CHUNK_SIZE as u64).min(end);
            let bytes = store.get_range(&key, offset..next).await?;
            Ok::<_, crate::storage::StorageError>(Some((bytes, (store, key, next, end))))
        },
    );
    let mut response = (
        status,
        Body::from_stream(stream.map_err(std::io::Error::other)),
    )
        .into_response();
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    response
        .headers_mut()
        .insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    if let Ok(value) = HeaderValue::from_str(&(end - start).to_string()) {
        response.headers_mut().insert(header::CONTENT_LENGTH, value);
    }
    if status == StatusCode::PARTIAL_CONTENT {
        let value = format!("bytes {start}-{}/{size}", end - 1);
        if let Ok(value) = HeaderValue::from_str(&value) {
            response.headers_mut().insert(header::CONTENT_RANGE, value);
        }
    }
    if let Some(name) = download_name {
        if let Ok(value) = HeaderValue::from_str(&format!("attachment; filename=\"{name}\"")) {
            response
                .headers_mut()
                .insert(header::CONTENT_DISPOSITION, value);
        }
    }
    response
}

fn parse_range(value: &str, size: u64) -> Option<(u64, u64)> {
    let range = value.strip_prefix("bytes=")?;
    if range.contains(',') || size == 0 {
        return None;
    }
    let (start, end) = range.split_once('-')?;
    if start.is_empty() {
        let suffix: u64 = end.parse().ok()?;
        if suffix == 0 {
            return None;
        }
        return Some((size.saturating_sub(suffix.min(size)), size));
    }
    let start: u64 = start.parse().ok()?;
    if start >= size {
        return None;
    }
    let end = if end.is_empty() {
        size
    } else {
        end.parse::<u64>().ok()?.saturating_add(1).min(size)
    };
    (start < end).then_some((start, end))
}

fn select_target(
    source_codec: &str,
    source_bitrate: u32,
    requested_format: Option<&str>,
    max_bitrate: Option<u32>,
    default_format: &str,
    default_bitrate: u32,
) -> (String, u32) {
    let requested = requested_format.map(str::to_ascii_lowercase);
    if requested.as_deref() == Some("raw") || (requested.is_none() && max_bitrate == Some(0)) {
        return ("raw".to_string(), 0);
    }
    let format = match requested.as_deref() {
        None if max_bitrate.is_none() => return ("raw".to_string(), 0),
        None => default_format.to_ascii_lowercase(),
        Some(format)
            if format == source_codec
                && max_bitrate.is_none_or(|limit| limit == 0 || source_bitrate <= limit) =>
        {
            format.to_string()
        }
        Some("aac" | "opus") => requested.expect("已匹配 Some"),
        Some(_) => default_format.to_ascii_lowercase(),
    };
    if format == source_codec {
        return (format, max_bitrate.unwrap_or(0));
    }
    let bitrate = match max_bitrate {
        Some(0) | None => default_bitrate,
        Some(limit) => default_bitrate.min(limit),
    };
    (format, bitrate)
}

#[cfg(test)]
mod tests {
    use super::select_target;

    #[test]
    fn format_only_and_zero_bitrate_use_safe_defaults() {
        assert_eq!(
            select_target("flac", 900, Some("opus"), None, "opus", 128),
            ("opus".into(), 128)
        );
        assert_eq!(
            select_target("flac", 900, None, Some(0), "opus", 128),
            ("raw".into(), 0)
        );
        assert_eq!(
            select_target("flac", 900, Some("mp3"), None, "opus", 128),
            ("opus".into(), 128)
        );
        assert_eq!(
            select_target("mp3", 320, Some("mp3"), Some(128), "opus", 128),
            ("opus".into(), 128)
        );
    }
}
