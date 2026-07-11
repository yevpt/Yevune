//! 为 `contract` 中既有 DTO 声明 UniFFI 外部记录，避免复制跨端数据模型。

use contract::{Album, Artist, Track};

#[uniffi::remote(Record)]
pub struct Album {
    pub id: String,
    pub name: String,
    pub artist: Option<String>,
    pub artist_id: Option<String>,
    pub cover_art: Option<String>,
    pub song_count: u32,
    pub duration: u32,
    pub year: Option<u32>,
    pub genre: Option<String>,
    pub created: Option<String>,
}

#[uniffi::remote(Record)]
pub struct Artist {
    pub id: String,
    pub name: String,
    pub sort_name: Option<String>,
    pub cover_art: Option<String>,
    pub music_brainz_id: Option<String>,
    pub album_count: u32,
}

#[uniffi::remote(Record)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub album: Option<String>,
    pub album_id: Option<String>,
    pub artist: Option<String>,
    pub artist_id: Option<String>,
    pub track: Option<u32>,
    pub disc_number: Option<u32>,
    pub year: Option<u32>,
    pub genre: Option<String>,
    pub cover_art: Option<String>,
    pub size: u64,
    pub content_type: Option<String>,
    pub suffix: Option<String>,
    pub duration: u32,
    pub bit_rate: u32,
    pub created: Option<String>,
}
