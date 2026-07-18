//! OpenSubsonic 结构化歌词端点。

use contract::StructuredLyrics;
use serde::Deserialize;

use crate::auth::AuthenticatedSession;
use crate::error::Result;
use crate::http::HttpClient;

pub(crate) async fn get_lyrics_by_song_id(
    http: &HttpClient,
    auth: &AuthenticatedSession,
    id: String,
) -> Result<Vec<StructuredLyrics>> {
    let payload: LyricsPayload = http
        .get_json(auth, "getLyricsBySongId", &[("id".to_owned(), id)])
        .await?;
    Ok(payload.lyrics_list.structured_lyrics)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LyricsPayload {
    lyrics_list: LyricsList,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LyricsList {
    #[serde(default)]
    structured_lyrics: Vec<StructuredLyrics>,
}
