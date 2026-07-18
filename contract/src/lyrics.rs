//! OpenSubsonic 结构化歌词 DTO。

use serde::{Deserialize, Serialize};

/// 一行歌词；同步歌词的 `start` 为相对歌曲开头的毫秒数。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LyricLine {
    pub start: Option<u64>,
    pub value: String,
}

/// 一份结构化歌词，对齐 OpenSubsonic `structuredLyrics`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StructuredLyrics {
    pub display_artist: Option<String>,
    pub display_title: Option<String>,
    pub lang: Option<String>,
    pub offset: i64,
    pub synced: bool,
    #[serde(rename = "line")]
    pub lines: Vec<LyricLine>,
}
