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
    /// 当前用户收藏的专辑。
    Starred,
}

impl AlbumSort {
    fn endpoint_value(self) -> &'static str {
        match self {
            Self::Newest => "newest",
            Self::AlphabeticalByName => "alphabeticalByName",
            Self::AlphabeticalByArtist => "alphabeticalByArtist",
            Self::Frequent => "frequent",
            Self::Recent => "recent",
            Self::Starred => "starred",
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

/// 当前用户收藏的艺人、专辑和歌曲。
#[derive(Clone, uniffi::Record)]
pub struct StarredCollection {
    pub artists: Vec<Artist>,
    pub albums: Vec<Album>,
    pub tracks: Vec<Track>,
}

/// `search3` 三类结果的独立分页请求。
#[derive(Clone, uniffi::Record)]
pub struct SearchPageRequest {
    /// 搜索关键字。
    pub query: String,
    /// 艺人结果偏移量。
    pub artist_offset: u32,
    /// 艺人结果数量。
    pub artist_count: u32,
    /// 专辑结果偏移量。
    pub album_offset: u32,
    /// 专辑结果数量。
    pub album_count: u32,
    /// 曲目结果偏移量。
    pub track_offset: u32,
    /// 曲目结果数量。
    pub track_count: u32,
}

/// `search3` 三类结果的独立分页响应。
#[derive(Clone, uniffi::Record)]
pub struct SearchPage {
    /// 当前页的艺人。
    pub artists: Vec<Artist>,
    /// 当前页的专辑。
    pub albums: Vec<Album>,
    /// 当前页的曲目。
    pub tracks: Vec<Track>,
    /// 是否还有更多艺人。
    pub has_more_artists: bool,
    /// 是否还有更多专辑。
    pub has_more_albums: bool,
    /// 是否还有更多曲目。
    pub has_more_tracks: bool,
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

pub(crate) async fn get_starred(
    http: &HttpClient,
    auth: &AuthenticatedSession,
) -> Result<StarredCollection> {
    let payload: StarredResponse = http.get_json(auth, "getStarred2", &[]).await?;
    Ok(StarredCollection {
        artists: payload.starred2.artist,
        albums: payload.starred2.album,
        tracks: payload.starred2.song,
    })
}

pub(crate) async fn search(
    http: &HttpClient,
    auth: &AuthenticatedSession,
    query: String,
) -> Result<SearchResult> {
    let page = search_page(
        http,
        auth,
        SearchPageRequest {
            query,
            artist_offset: 0,
            artist_count: 20,
            album_offset: 0,
            album_count: 20,
            track_offset: 0,
            track_count: 20,
        },
    )
    .await?;
    Ok(SearchResult {
        artists: page.artists,
        albums: page.albums,
        tracks: page.tracks,
    })
}

pub(crate) async fn search_page(
    http: &HttpClient,
    auth: &AuthenticatedSession,
    request: SearchPageRequest,
) -> Result<SearchPage> {
    let requested_count = |count| if count == 0 { 0 } else { count + 1 };
    let payload: SearchResponse = http
        .get_json(
            auth,
            "search3",
            &[
                ("query".to_owned(), request.query),
                ("artistOffset".to_owned(), request.artist_offset.to_string()),
                (
                    "artistCount".to_owned(),
                    requested_count(request.artist_count).to_string(),
                ),
                ("albumOffset".to_owned(), request.album_offset.to_string()),
                (
                    "albumCount".to_owned(),
                    requested_count(request.album_count).to_string(),
                ),
                ("songOffset".to_owned(), request.track_offset.to_string()),
                (
                    "songCount".to_owned(),
                    requested_count(request.track_count).to_string(),
                ),
            ],
        )
        .await?;
    let (artists, has_more_artists) =
        trim_page(payload.search_result3.artist, request.artist_count);
    let (albums, has_more_albums) = trim_page(payload.search_result3.album, request.album_count);
    let (tracks, has_more_tracks) = trim_page(payload.search_result3.song, request.track_count);
    Ok(SearchPage {
        artists,
        albums,
        tracks,
        has_more_artists,
        has_more_albums,
        has_more_tracks,
    })
}

fn trim_page<T>(mut values: Vec<T>, count: u32) -> (Vec<T>, bool) {
    let limit = count as usize;
    let has_more = values.len() > limit;
    values.truncate(limit);
    (values, has_more)
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
struct StarredResponse {
    starred2: StarredBody,
}

#[derive(Deserialize)]
struct StarredBody {
    #[serde(default)]
    artist: Vec<Artist>,
    #[serde(default)]
    album: Vec<Album>,
    #[serde(default)]
    song: Vec<Track>,
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
