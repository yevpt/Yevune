//! 曲库浏览与全文搜索端点。

use contract::{Album, Artist, Genre, Track};
use serde::Deserialize;

use crate::auth::AuthenticatedSession;
use crate::error::Result;
use crate::http::HttpClient;

/// `getAlbumList2` 支持的列表排序方式。
#[derive(Clone, Copy, uniffi::Enum)]
pub enum AlbumSort {
    /// 最近入库。
    Newest,
    /// 按专辑名。
    AlphabeticalByName,
    /// 按艺人名。
    AlphabeticalByArtist,
    /// 最常播放。
    Frequent,
    /// 最近播放。
    Recent,
}

impl AlbumSort {
    fn endpoint_value(self) -> &'static str {
        match self {
            Self::Newest => "newest",
            Self::AlphabeticalByName => "alphabeticalByName",
            Self::AlphabeticalByArtist => "alphabeticalByArtist",
            Self::Frequent => "frequent",
            Self::Recent => "recent",
        }
    }
}

/// `getAlbumList2` 的查询意图：三态互斥——按排序、按流派、按年份区间。
#[derive(Clone, uniffi::Enum)]
pub enum AlbumFilter {
    /// 按既有排序方式浏览。
    Sort(AlbumSort),
    /// 按流派筛选（对应 `type=byGenre&genre=`）。
    Genre(String),
    /// 按年份区间筛选，闭区间（对应 `type=byYear&fromYear=&toYear=`）。
    YearRange {
        /// 起始年份（含）。
        from: u32,
        /// 结束年份（含）。
        to: u32,
    },
}

impl AlbumFilter {
    fn query_params(&self) -> Vec<(String, String)> {
        match self {
            Self::Sort(sort) => vec![("type".to_owned(), sort.endpoint_value().to_owned())],
            Self::Genre(genre) => vec![
                ("type".to_owned(), "byGenre".to_owned()),
                ("genre".to_owned(), genre.clone()),
            ],
            Self::YearRange { from, to } => vec![
                ("type".to_owned(), "byYear".to_owned()),
                ("fromYear".to_owned(), from.to_string()),
                ("toYear".to_owned(), to.to_string()),
            ],
        }
    }
}

/// 包含专辑及其曲目的详情。
#[derive(Clone, uniffi::Record)]
pub struct AlbumDetail {
    /// 专辑元数据。
    pub album: Album,
    /// 专辑中的可见曲目。
    pub tracks: Vec<Track>,
}

/// 包含艺人及其专辑的详情。
#[derive(Clone, uniffi::Record)]
pub struct ArtistDetail {
    /// 艺人元数据。
    pub artist: Artist,
    /// 艺人名下的可见专辑。
    pub albums: Vec<Album>,
}

/// `search3` 的三类命中。
#[derive(Clone, uniffi::Record)]
pub struct SearchResult {
    /// 匹配的艺人。
    pub artists: Vec<Artist>,
    /// 匹配的专辑。
    pub albums: Vec<Album>,
    /// 匹配的曲目。
    pub tracks: Vec<Track>,
}

pub(crate) async fn list_albums(
    http: &HttpClient,
    auth: &AuthenticatedSession,
    filter: AlbumFilter,
    offset: u32,
    size: u32,
) -> Result<Vec<Album>> {
    let mut params = filter.query_params();
    params.push(("offset".to_owned(), offset.to_string()));
    params.push(("size".to_owned(), size.to_string()));
    let payload: AlbumListPayload = http.get_json(auth, "getAlbumList2", &params).await?;
    Ok(payload.album_list2.album)
}

pub(crate) async fn list_genres(
    http: &HttpClient,
    auth: &AuthenticatedSession,
) -> Result<Vec<Genre>> {
    let payload: GenresPayload = http.get_json(auth, "getGenres", &[]).await?;
    Ok(payload.genres.genre)
}

pub(crate) async fn get_album(
    http: &HttpClient,
    auth: &AuthenticatedSession,
    id: String,
) -> Result<AlbumDetail> {
    let payload: AlbumResponse = http
        .get_json(auth, "getAlbum", &[("id".to_owned(), id)])
        .await?;
    Ok(AlbumDetail {
        album: payload.album.album,
        tracks: payload.album.song,
    })
}

pub(crate) async fn get_artist(
    http: &HttpClient,
    auth: &AuthenticatedSession,
    id: String,
) -> Result<ArtistDetail> {
    let payload: ArtistResponse = http
        .get_json(auth, "getArtist", &[("id".to_owned(), id)])
        .await?;
    Ok(ArtistDetail {
        artist: payload.artist.artist,
        albums: payload.artist.album,
    })
}

pub(crate) async fn get_song(
    http: &HttpClient,
    auth: &AuthenticatedSession,
    id: String,
) -> Result<Track> {
    let payload: SongResponse = http
        .get_json(auth, "getSong", &[("id".to_owned(), id)])
        .await?;
    Ok(payload.song)
}

pub(crate) async fn list_artists(
    http: &HttpClient,
    auth: &AuthenticatedSession,
) -> Result<Vec<Artist>> {
    let payload: ArtistsResponse = http.get_json(auth, "getArtists", &[]).await?;
    Ok(payload
        .artists
        .index
        .into_iter()
        .flat_map(|index| index.artist)
        .collect())
}

pub(crate) async fn search(
    http: &HttpClient,
    auth: &AuthenticatedSession,
    query: String,
) -> Result<SearchResult> {
    let payload: SearchResponse = http
        .get_json(auth, "search3", &[("query".to_owned(), query)])
        .await?;
    Ok(SearchResult {
        artists: payload.search_result3.artist,
        albums: payload.search_result3.album,
        tracks: payload.search_result3.song,
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AlbumListPayload {
    album_list2: AlbumList,
}

#[derive(Deserialize)]
struct AlbumList {
    #[serde(default)]
    album: Vec<Album>,
}

#[derive(Deserialize)]
struct AlbumResponse {
    album: AlbumWithTracks,
}

#[derive(Deserialize)]
struct AlbumWithTracks {
    #[serde(flatten)]
    album: Album,
    #[serde(default)]
    song: Vec<Track>,
}

#[derive(Deserialize)]
struct ArtistResponse {
    artist: ArtistWithAlbums,
}

#[derive(Deserialize)]
struct ArtistWithAlbums {
    #[serde(flatten)]
    artist: Artist,
    #[serde(default)]
    album: Vec<Album>,
}

#[derive(Deserialize)]
struct SongResponse {
    song: Track,
}

#[derive(Deserialize)]
struct GenresPayload {
    genres: GenresList,
}

#[derive(Deserialize)]
struct GenresList {
    #[serde(default)]
    genre: Vec<Genre>,
}

#[derive(Deserialize)]
struct ArtistsResponse {
    artists: Artists,
}

#[derive(Deserialize)]
struct Artists {
    #[serde(default)]
    index: Vec<ArtistIndex>,
}

#[derive(Deserialize)]
struct ArtistIndex {
    #[serde(default)]
    artist: Vec<Artist>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchResponse {
    search_result3: SearchPayload,
}

#[derive(Deserialize)]
struct SearchPayload {
    #[serde(default)]
    artist: Vec<Artist>,
    #[serde(default)]
    album: Vec<Album>,
    #[serde(default)]
    song: Vec<Track>,
}
