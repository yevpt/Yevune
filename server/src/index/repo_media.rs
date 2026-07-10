//! 媒体仓储：艺人/专辑/曲目的 upsert、读取、列举与 FTS5 搜索。
//!
//! 内部主键为 `i64`，对外返回 contract DTO（不透明 `String` id）。

use contract::{Album, Artist, Track};
use sqlx::{FromRow, SqlitePool};

use super::Result;

/// 专辑读取行（含艺人名与聚合）。
#[derive(FromRow)]
struct AlbumRow {
    id: i64,
    name: String,
    artist_id: Option<i64>,
    artist_name: Option<String>,
    year: Option<i64>,
    genre: Option<String>,
    cover_key: Option<String>,
    added_at: String,
    song_count: i64,
    duration: i64,
}

impl From<AlbumRow> for Album {
    fn from(r: AlbumRow) -> Self {
        Album {
            id: r.id.to_string(),
            name: r.name,
            artist: r.artist_name,
            artist_id: r.artist_id.map(|i| i.to_string()),
            cover_art: r.cover_key,
            song_count: r.song_count as u32,
            duration: r.duration as u32,
            year: r.year.map(|i| i as u32),
            genre: r.genre,
            created: Some(r.added_at),
        }
    }
}

/// 曲目读取行（含关联专辑/艺人名与封面）。
#[derive(FromRow)]
struct TrackRow {
    id: i64,
    title: String,
    album_id: Option<i64>,
    album_name: Option<String>,
    artist_id: Option<i64>,
    artist_name: Option<String>,
    disc_no: Option<i64>,
    track_no: Option<i64>,
    year: Option<i64>,
    genre: Option<String>,
    duration: Option<i64>,
    codec: Option<String>,
    bitrate: Option<i64>,
    size: Option<i64>,
    cover_key: Option<String>,
    added_at: String,
}

impl From<TrackRow> for Track {
    fn from(r: TrackRow) -> Self {
        Track {
            id: r.id.to_string(),
            title: r.title,
            album: r.album_name,
            album_id: r.album_id.map(|i| i.to_string()),
            artist: r.artist_name,
            artist_id: r.artist_id.map(|i| i.to_string()),
            track: r.track_no.map(|i| i as u32),
            disc_number: r.disc_no.map(|i| i as u32),
            year: r.year.map(|i| i as u32),
            genre: r.genre,
            cover_art: r.cover_key,
            size: r.size.unwrap_or(0) as u64,
            content_type: None,
            suffix: r.codec,
            duration: r.duration.unwrap_or(0) as u32,
            bit_rate: r.bitrate.unwrap_or(0) as u32,
            created: Some(r.added_at),
        }
    }
}

/// 艺人读取行。
#[derive(FromRow)]
struct ArtistRow {
    id: i64,
    name: String,
    sort_name: Option<String>,
    mbid: Option<String>,
    cover_key: Option<String>,
    album_count: i64,
}

impl From<ArtistRow> for Artist {
    fn from(r: ArtistRow) -> Self {
        Artist {
            id: r.id.to_string(),
            name: r.name,
            sort_name: r.sort_name,
            cover_art: r.cover_key,
            music_brainz_id: r.mbid,
            album_count: r.album_count as u32,
        }
    }
}

/// 取单曲目的 SELECT（含 LEFT JOIN 专辑/艺人）。
const TRACK_SELECT: &str = "\
SELECT t.id, t.title, t.album_id, a.name AS album_name, \
       t.artist_id, ar.name AS artist_name, t.disc_no, t.track_no, t.year, \
       t.genre, t.duration, t.codec, t.bitrate, t.size, a.cover_key AS cover_key, t.added_at \
FROM tracks t \
LEFT JOIN albums a ON t.album_id = a.id \
LEFT JOIN artists ar ON t.artist_id = ar.id";

/// 取专辑的 SELECT（含艺人名与曲目聚合）。
const ALBUM_SELECT: &str = "\
SELECT a.id, a.name, a.artist_id, ar.name AS artist_name, a.year, a.genre, \
       a.cover_key, a.added_at, \
       COUNT(t.id) AS song_count, COALESCE(SUM(t.duration), 0) AS duration \
FROM albums a \
LEFT JOIN artists ar ON a.artist_id = ar.id \
LEFT JOIN tracks t ON t.album_id = a.id";

/// 取艺人的 SELECT（含专辑数聚合）。
const ARTIST_SELECT: &str = "\
SELECT ar.id, ar.name, ar.sort_name, ar.mbid, ar.cover_key, \
       COUNT(al.id) AS album_count \
FROM artists ar \
LEFT JOIN albums al ON al.artist_id = ar.id";

/// 入库/更新一条曲目所需的字段（内部关联用 `i64` 外键）。
#[derive(Debug, Clone, Default)]
pub struct NewTrack {
    /// 标题。
    pub title: String,
    /// 所属专辑主键。
    pub album_id: Option<i64>,
    /// 艺人主键。
    pub artist_id: Option<i64>,
    /// 碟片号。
    pub disc_no: Option<u32>,
    /// 曲目号。
    pub track_no: Option<u32>,
    /// 年份。
    pub year: Option<u32>,
    /// 流派。
    pub genre: Option<String>,
    /// 时长（秒）。
    pub duration: Option<u32>,
    /// 编码，如 `flac`。
    pub codec: Option<String>,
    /// 码率（kbps）。
    pub bitrate: Option<u32>,
    /// 文件大小（字节）。
    pub size: Option<u64>,
    /// Garage 原始文件键（唯一）。
    pub object_key: String,
    /// 变更检测 ETag。
    pub etag: Option<String>,
    /// 内容哈希。
    pub content_hash: Option<String>,
    /// ReplayGain。
    pub replaygain: Option<f64>,
}

/// search3 的搜索结果，按类型分组。
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SearchResults {
    /// 命中的艺人。
    pub artists: Vec<Artist>,
    /// 命中的专辑。
    pub albums: Vec<Album>,
    /// 命中的曲目。
    pub tracks: Vec<Track>,
}

/// 媒体仓储。
pub struct MediaRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> MediaRepo<'a> {
    /// 绑定连接池。
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// 按名 upsert 艺人，返回主键（已存在则复用）。
    pub async fn upsert_artist(&self, name: &str) -> Result<i64> {
        sqlx::query_scalar(
            "INSERT INTO artists(name) VALUES(?) \
             ON CONFLICT(name) DO UPDATE SET name = excluded.name RETURNING id",
        )
        .bind(name)
        .fetch_one(self.pool)
        .await
    }

    /// 按 (名, 艺人) upsert 专辑，返回主键。
    pub async fn upsert_album(
        &self,
        name: &str,
        artist_id: Option<i64>,
        year: Option<u32>,
        genre: Option<&str>,
    ) -> Result<i64> {
        sqlx::query_scalar(
            "INSERT INTO albums(name, artist_id, year, genre) VALUES(?, ?, ?, ?) \
             ON CONFLICT(name, artist_id) DO UPDATE SET \
             year = excluded.year, genre = excluded.genre RETURNING id",
        )
        .bind(name)
        .bind(artist_id)
        .bind(year.map(|v| v as i64))
        .bind(genre)
        .fetch_one(self.pool)
        .await
    }

    /// 按 object_key upsert 曲目，返回主键。
    pub async fn upsert_track(&self, track: &NewTrack) -> Result<i64> {
        sqlx::query_scalar(
            "INSERT INTO tracks(title, album_id, artist_id, disc_no, track_no, year, genre, \
                 duration, codec, bitrate, size, object_key, etag, content_hash, replaygain) \
             VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(object_key) DO UPDATE SET \
                 title = excluded.title, album_id = excluded.album_id, \
                 artist_id = excluded.artist_id, disc_no = excluded.disc_no, \
                 track_no = excluded.track_no, year = excluded.year, genre = excluded.genre, \
                 duration = excluded.duration, codec = excluded.codec, \
                 bitrate = excluded.bitrate, size = excluded.size, etag = excluded.etag, \
                 content_hash = excluded.content_hash, replaygain = excluded.replaygain \
             RETURNING id",
        )
        .bind(&track.title)
        .bind(track.album_id)
        .bind(track.artist_id)
        .bind(track.disc_no.map(|v| v as i64))
        .bind(track.track_no.map(|v| v as i64))
        .bind(track.year.map(|v| v as i64))
        .bind(&track.genre)
        .bind(track.duration.map(|v| v as i64))
        .bind(&track.codec)
        .bind(track.bitrate.map(|v| v as i64))
        .bind(track.size.map(|v| v as i64))
        .bind(&track.object_key)
        .bind(&track.etag)
        .bind(&track.content_hash)
        .bind(track.replaygain)
        .fetch_one(self.pool)
        .await
    }

    /// 按主键取曲目 DTO。
    pub async fn get_track(&self, id: i64) -> Result<Option<Track>> {
        let row: Option<TrackRow> = sqlx::query_as(&format!("{TRACK_SELECT} WHERE t.id = ?"))
            .bind(id)
            .fetch_optional(self.pool)
            .await?;
        Ok(row.map(Track::from))
    }

    /// 按主键取专辑 DTO。
    pub async fn get_album(&self, id: i64) -> Result<Option<Album>> {
        let row: Option<AlbumRow> =
            sqlx::query_as(&format!("{ALBUM_SELECT} WHERE a.id = ? GROUP BY a.id"))
                .bind(id)
                .fetch_optional(self.pool)
                .await?;
        Ok(row.map(Album::from))
    }

    /// 列举全部专辑（按名排序）。
    pub async fn list_albums(&self) -> Result<Vec<Album>> {
        let rows: Vec<AlbumRow> =
            sqlx::query_as(&format!("{ALBUM_SELECT} GROUP BY a.id ORDER BY a.name"))
                .fetch_all(self.pool)
                .await?;
        Ok(rows.into_iter().map(Album::from).collect())
    }

    /// FTS5 搜索艺人/专辑/曲目名，按类型分组返回。
    pub async fn search(&self, query: &str, limit: i64) -> Result<SearchResults> {
        // 命中的 (kind, ref_id)
        let hits: Vec<(String, i64)> =
            sqlx::query_as("SELECT kind, ref_id FROM search_fts WHERE search_fts MATCH ? LIMIT ?")
                .bind(query)
                .bind(limit)
                .fetch_all(self.pool)
                .await?;

        let mut results = SearchResults::default();
        for (kind, ref_id) in hits {
            match kind.as_str() {
                "artist" => {
                    if let Some(a) = self.get_artist(ref_id).await? {
                        results.artists.push(a);
                    }
                }
                "album" => {
                    if let Some(a) = self.get_album(ref_id).await? {
                        results.albums.push(a);
                    }
                }
                "track" => {
                    if let Some(t) = self.get_track(ref_id).await? {
                        results.tracks.push(t);
                    }
                }
                _ => {}
            }
        }
        Ok(results)
    }

    /// 按主键取艺人 DTO。
    pub async fn get_artist(&self, id: i64) -> Result<Option<Artist>> {
        let row: Option<ArtistRow> =
            sqlx::query_as(&format!("{ARTIST_SELECT} WHERE ar.id = ? GROUP BY ar.id"))
                .bind(id)
                .fetch_optional(self.pool)
                .await?;
        Ok(row.map(Artist::from))
    }

    /// 按 object_key 删除曲目，返回是否删除到行。
    pub async fn delete_by_object_key(&self, object_key: &str) -> Result<bool> {
        let affected = sqlx::query("DELETE FROM tracks WHERE object_key = ?")
            .bind(object_key)
            .execute(self.pool)
            .await?
            .rows_affected();
        Ok(affected > 0)
    }
}
