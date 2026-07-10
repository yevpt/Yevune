//! 标签解析：从**文件头字节**（经 [`ObjectStore::get_range`] 有界读取，非整文件）
//! 用 `lofty` 解析音频标签、时长与内嵌封面。
//!
//! FLAC/ID3 的元数据块位于音频帧之前，因此只需文件头即可解析——满足红线「绝不整读音频」。
//!
//! [`ObjectStore::get_range`]: crate::storage::ObjectStore::get_range

use std::io::Cursor;

use bytes::Bytes;
use lofty::file::{AudioFile, FileType, TaggedFileExt};
use lofty::prelude::{Accessor, ItemKey};
use lofty::probe::Probe;
use lofty::tag::Tag;

/// 内嵌封面的原始字节与 MIME。
#[derive(Debug, Clone)]
pub struct ParsedCover {
    /// 封面图片原始字节。
    pub data: Bytes,
    /// MIME 类型（如 `image/jpeg`），可能缺省。
    pub mime: Option<String>,
}

/// 从文件头解析出的曲目元数据。
#[derive(Debug, Default, Clone)]
pub struct ParsedTrack {
    /// 标题。
    pub title: Option<String>,
    /// 艺人。
    pub artist: Option<String>,
    /// 专辑名。
    pub album: Option<String>,
    /// 专辑艺人（用于合辑归类，暂记录不入专用列）。
    pub album_artist: Option<String>,
    /// 曲目号。
    pub track_no: Option<u32>,
    /// 碟片号。
    pub disc_no: Option<u32>,
    /// 年份。
    pub year: Option<u32>,
    /// 流派。
    pub genre: Option<String>,
    /// 时长（秒），来自 STREAMINFO/头部属性。
    pub duration_secs: Option<u32>,
    /// 编码短名（如 `flac`）。
    pub codec: Option<String>,
    /// 内嵌封面（若有）。
    pub cover: Option<ParsedCover>,
}

/// 解析文件头字节为 [`ParsedTrack`]。`bytes` 应为对象前若干字节（含全部元数据块）。
pub fn parse_header(bytes: Bytes) -> Result<ParsedTrack, String> {
    let cursor = Cursor::new(bytes);
    let tagged = Probe::new(cursor)
        .guess_file_type()
        .map_err(|e| e.to_string())?
        .read()
        .map_err(|e| e.to_string())?;

    let codec = codec_name(tagged.file_type());
    let duration = tagged.properties().duration();
    let duration_secs = if duration.is_zero() {
        None
    } else {
        Some(duration.as_secs() as u32)
    };

    let mut parsed = ParsedTrack {
        duration_secs,
        codec,
        ..Default::default()
    };

    if let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) {
        parsed.title = tag.title().map(|s| s.to_string());
        parsed.artist = tag.artist().map(|s| s.to_string());
        parsed.album = tag.album().map(|s| s.to_string());
        parsed.album_artist = tag.get_string(ItemKey::AlbumArtist).map(|s| s.to_string());
        parsed.track_no = tag.track();
        parsed.disc_no = tag.disk();
        parsed.year = parse_year(tag);
        parsed.genre = tag.genre().map(|s| s.to_string());
        if let Some(pic) = tag.pictures().first() {
            parsed.cover = Some(ParsedCover {
                data: Bytes::copy_from_slice(pic.data()),
                mime: pic.mime_type().map(|m| m.as_str().to_string()),
            });
        }
    }

    Ok(parsed)
}

/// 从标签解析年份：优先 `YEAR`，回退到日期字段（如 Vorbis `DATE`），取前 4 位。
fn parse_year(tag: &Tag) -> Option<u32> {
    for key in [ItemKey::Year, ItemKey::RecordingDate] {
        if let Some(s) = tag.get_string(key) {
            if let Some(year) = s.get(0..4).and_then(|p| p.parse::<u32>().ok()) {
                return Some(year);
            }
        }
    }
    None
}

/// 文件类型 → 编码短名（用于 `tracks.codec`）。
fn codec_name(ft: FileType) -> Option<String> {
    let name = match ft {
        FileType::Flac => "flac",
        FileType::Mpeg => "mp3",
        FileType::Mp4 => "m4a",
        FileType::Opus => "opus",
        FileType::Vorbis => "ogg",
        FileType::Wav => "wav",
        FileType::Aac => "aac",
        FileType::Ape => "ape",
        FileType::WavPack => "wv",
        _ => return None,
    };
    Some(name.to_string())
}
