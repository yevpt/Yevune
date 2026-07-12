//! 媒体类型：艺人、专辑、曲目、流派。字段对齐 OpenSubsonic `getArtist`/`getAlbum`/`getSong`。

use serde::{Deserialize, Serialize};

/// 流派，对齐 OpenSubsonic `getGenres` 的 genre 元素。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Genre {
    /// 流派名（即取值）。
    pub value: String,
    /// 该流派下曲目数。
    pub song_count: u32,
    /// 该流派下专辑数。
    pub album_count: u32,
}

/// 艺人，对齐 OpenSubsonic `ArtistID3` + 设计文档 §6 `artists` 列。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Artist {
    /// 不透明标识符。
    pub id: String,
    /// 展示名。
    pub name: String,
    /// 排序名（可空）。
    pub sort_name: Option<String>,
    /// 封面标识（对应内部 `cover_key`，客户端凭此调 `getCoverArt`）。
    pub cover_art: Option<String>,
    /// MusicBrainz ID（可空）。
    pub music_brainz_id: Option<String>,
    /// 专辑数。
    pub album_count: u32,
}

/// 专辑，对齐 OpenSubsonic `AlbumID3` + 设计文档 §6 `albums` 列。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Album {
    /// 不透明标识符。
    pub id: String,
    /// 专辑名。
    pub name: String,
    /// 艺人名（冗余，便于展示）。
    pub artist: Option<String>,
    /// 艺人标识符。
    pub artist_id: Option<String>,
    /// 封面标识。
    pub cover_art: Option<String>,
    /// 曲目数。
    pub song_count: u32,
    /// 总时长（秒）。
    pub duration: u32,
    /// 发行年份。
    pub year: Option<u32>,
    /// 流派名。
    pub genre: Option<String>,
    /// 入库时间（ISO8601），对应 `added_at`。
    pub created: Option<String>,
}

/// 曲目，对齐 OpenSubsonic `Child`(song) + 设计文档 §6 `tracks` 列。
///
/// 面向客户端视图：不含 `object_key`/`etag`/`content_hash` 等对象存储内部字段。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    /// 不透明标识符。
    pub id: String,
    /// 标题。
    pub title: String,
    /// 所属专辑名。
    pub album: Option<String>,
    /// 所属专辑标识符。
    pub album_id: Option<String>,
    /// 艺人名。
    pub artist: Option<String>,
    /// 艺人标识符。
    pub artist_id: Option<String>,
    /// 曲目号，对应 `track_no`。
    pub track: Option<u32>,
    /// 碟片号，对应 `disc_no`。
    pub disc_number: Option<u32>,
    /// 发行年份。
    pub year: Option<u32>,
    /// 流派名。
    pub genre: Option<String>,
    /// 封面标识。
    pub cover_art: Option<String>,
    /// 文件大小（字节）。
    pub size: u64,
    /// MIME 类型，如 `audio/flac`。
    pub content_type: Option<String>,
    /// 文件后缀/编码，如 `flac`。
    pub suffix: Option<String>,
    /// 时长（秒）。
    pub duration: u32,
    /// 码率（kbps）。
    pub bit_rate: u32,
    /// 入库时间（ISO8601），对应 `added_at`。
    pub created: Option<String>,
    /// Garage 原始对象键（`library/...`），OpenSubsonic `path`；客户端整理/移动时的当前定位。
    pub path: Option<String>,
}
