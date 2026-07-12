//! 歌单与歌单文件夹（自研多级歌单模型，设计文档 §6）。
//!
//! 文件夹只作容器，歌单是装曲目的叶子；每个用户拥有各自独立的树（`owner_id` 隔离）。

use serde::{Deserialize, Serialize};

/// 歌单文件夹（容器节点），对应 `playlist_folders` 表。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistFolder {
    /// 不透明标识符。
    pub id: String,
    /// 所有者用户标识符（隔离各用户的树）。
    pub owner_id: String,
    /// 文件夹名。
    pub name: String,
    /// 父文件夹标识符；`None` 表示顶级。
    pub parent_id: Option<String>,
    /// 在同级中的位置。
    pub position: u32,
}

/// 歌单（叶子节点），对应 `playlists` 表，默认私有。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Playlist {
    /// 不透明标识符。
    pub id: String,
    /// 所有者用户标识符。标准 OpenSubsonic `getPlaylist`/`createPlaylist` 响应不含此字段
    /// （只在扩展的 `getPlaylistTree` 输出），故解码时缺省为空串。
    #[serde(default)]
    pub owner_id: String,
    /// 歌单名。
    pub name: String,
    /// 备注。
    pub comment: Option<String>,
    /// 所属文件夹标识符；`None` 表示根级。
    pub folder_id: Option<String>,
    /// 在同级中的位置。标准 OpenSubsonic 响应不含此字段（只在扩展树输出），故解码时缺省为 0。
    #[serde(default)]
    pub position: u32,
    /// 曲目数。
    pub song_count: u32,
    /// 总时长（秒）。
    pub duration: u32,
    /// 创建时间（ISO8601）。
    pub created: Option<String>,
    /// 最近修改时间（ISO8601）。
    pub changed: Option<String>,
}
