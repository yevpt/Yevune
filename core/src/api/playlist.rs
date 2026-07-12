//! 当前用户多级歌单：树读取、详情、增删改查与组织。

use contract::{Playlist, PlaylistFolder, Track};
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

/// 歌单及其（经服务端访问控制过滤后的）曲目。
#[derive(Clone, uniffi::Record)]
pub struct PlaylistDetail {
    pub playlist: Playlist,
    pub tracks: Vec<Track>,
}

pub(crate) async fn playlist_detail(
    http: &HttpClient,
    auth: &AuthenticatedSession,
    id: String,
) -> Result<PlaylistDetail> {
    let payload: DetailPayload = http
        .get_json(auth, "getPlaylist", &[("id".to_owned(), id)])
        .await?;
    Ok(PlaylistDetail {
        playlist: payload.playlist.playlist,
        tracks: payload.playlist.entry,
    })
}

pub(crate) async fn create_playlist(
    http: &HttpClient,
    auth: &AuthenticatedSession,
    name: String,
    folder_id: Option<String>,
    song_ids: Vec<String>,
) -> Result<Playlist> {
    let mut params = vec![("name".to_owned(), name)];
    for song in song_ids {
        params.push(("songId".to_owned(), song));
    }
    let payload: DetailPayload = http.get_json(auth, "createPlaylist", &params).await?;
    let mut playlist = payload.playlist.playlist;
    if let Some(folder_id) = folder_id {
        move_playlist(http, auth, playlist.id.clone(), Some(folder_id.clone())).await?;
        playlist.folder_id = Some(folder_id);
    }
    Ok(playlist)
}

pub(crate) async fn move_playlist(
    http: &HttpClient,
    auth: &AuthenticatedSession,
    id: String,
    folder_id: Option<String>,
) -> Result<()> {
    let mut params = vec![("id".to_owned(), id)];
    if let Some(folder_id) = folder_id {
        params.push(("folderId".to_owned(), folder_id));
    }
    http.get_empty_with_params(auth, "ext/movePlaylist", &params)
        .await
}

pub(crate) async fn delete_playlist(
    http: &HttpClient,
    auth: &AuthenticatedSession,
    id: String,
) -> Result<()> {
    http.get_empty_with_params(auth, "deletePlaylist", &[("id".to_owned(), id)])
        .await
}

pub(crate) async fn rename_playlist(
    http: &HttpClient,
    auth: &AuthenticatedSession,
    id: String,
    name: String,
) -> Result<()> {
    http.get_empty_with_params(
        auth,
        "updatePlaylist",
        &[("playlistId".to_owned(), id), ("name".to_owned(), name)],
    )
    .await
}

pub(crate) async fn set_playlist_comment(
    http: &HttpClient,
    auth: &AuthenticatedSession,
    id: String,
    comment: String,
) -> Result<()> {
    http.get_empty_with_params(
        auth,
        "updatePlaylist",
        &[
            ("playlistId".to_owned(), id),
            ("comment".to_owned(), comment),
        ],
    )
    .await
}

pub(crate) async fn add_tracks(
    http: &HttpClient,
    auth: &AuthenticatedSession,
    id: String,
    song_ids: Vec<String>,
) -> Result<()> {
    let mut params = vec![("playlistId".to_owned(), id)];
    for song in song_ids {
        params.push(("songIdToAdd".to_owned(), song));
    }
    http.get_empty_with_params(auth, "updatePlaylist", &params)
        .await
}

pub(crate) async fn remove_track_at(
    http: &HttpClient,
    auth: &AuthenticatedSession,
    id: String,
    index: i64,
) -> Result<()> {
    http.get_empty_with_params(
        auth,
        "updatePlaylist",
        &[
            ("playlistId".to_owned(), id),
            ("songIndexToRemove".to_owned(), index.to_string()),
        ],
    )
    .await
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

#[derive(Deserialize)]
struct DetailPayload {
    playlist: PlaylistWithEntries,
}

#[derive(Deserialize)]
struct PlaylistWithEntries {
    #[serde(flatten)]
    playlist: Playlist,
    #[serde(default)]
    entry: Vec<Track>,
}
