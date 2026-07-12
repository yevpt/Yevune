//! 当前用户多级歌单：树读取、详情、增删改查与组织。

use contract::{Playlist, PlaylistFolder};
use serde::Deserialize;

use crate::auth::AuthenticatedSession;
use crate::error::Result;
use crate::http::HttpClient;

/// 一次性拿到的歌单文件夹树与叶子歌单，层级由 UI 本地组装。
#[derive(Clone, uniffi::Record)]
pub struct PlaylistTree {
    pub folders: Vec<PlaylistFolder>,
    pub playlists: Vec<Playlist>,
}

pub(crate) async fn playlist_tree(
    http: &HttpClient,
    auth: &AuthenticatedSession,
) -> Result<PlaylistTree> {
    let payload: TreePayload = http.get_json(auth, "ext/getPlaylistTree", &[]).await?;
    Ok(PlaylistTree {
        folders: payload.playlist_tree.folders,
        playlists: payload.playlist_tree.playlists,
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TreePayload {
    playlist_tree: TreeBody,
}

#[derive(Deserialize)]
struct TreeBody {
    #[serde(default)]
    folders: Vec<PlaylistFolder>,
    #[serde(default)]
    playlists: Vec<Playlist>,
}
