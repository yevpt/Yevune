//! 媒体仓储：艺人/专辑/曲目的 upsert、读取、列举与 FTS5 搜索。
//!
//! 内部主键为 `i64`，对外返回 contract DTO（不透明 `String` id）。

use contract::{Album, Artist, Track};
use sqlx::{FromRow, SqlitePool};

use super::{AccessControl, Result, Viewer};

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

    // ─────────── 可见性过滤读方法（曲库访问控制强制，设计文档 §6）───────────
    //
    // 把 [`AccessControl`] 的可见性谓词注入 SQL，在数据层统一强制"查询时评估 +
    // 最具体优先 + 管理员绕过"。专辑/艺人以"是否含至少一条可见曲目"收敛可见性，
    // 且聚合计数只计可见曲目，避免受限内容经计数泄漏。

    /// 生成作用于曲目别名 `alias` 的可见性谓词。
    fn visibility(&self, viewer: &Viewer, alias: &str) -> String {
        AccessControl::new(self.pool).visibility_sql_for(viewer, alias)
    }

    /// 按主键取曲目 DTO，仅当 `viewer` 可见。
    pub async fn get_track_visible(&self, viewer: &Viewer, id: i64) -> Result<Option<Track>> {
        let pred = self.visibility(viewer, "t");
        let row: Option<TrackRow> =
            sqlx::query_as(&format!("{TRACK_SELECT} WHERE t.id = ? AND ({pred})"))
                .bind(id)
                .fetch_optional(self.pool)
                .await?;
        Ok(row.map(Track::from))
    }

    /// 列举某专辑内 `viewer` 可见的曲目（按碟/曲序）。
    pub async fn album_tracks_visible(&self, viewer: &Viewer, album_id: i64) -> Result<Vec<Track>> {
        let pred = self.visibility(viewer, "t");
        let rows: Vec<TrackRow> = sqlx::query_as(&format!(
            "{TRACK_SELECT} WHERE t.album_id = ? AND ({pred}) \
             ORDER BY t.disc_no, t.track_no, t.id"
        ))
        .bind(album_id)
        .fetch_all(self.pool)
        .await?;
        Ok(rows.into_iter().map(Track::from).collect())
    }

    /// 列举 `viewer` 可见的专辑（含至少一条可见曲目；计数只计可见曲目）。
    pub async fn list_albums_visible(&self, viewer: &Viewer) -> Result<Vec<Album>> {
        let pred = self.visibility(viewer, "t");
        let rows: Vec<AlbumRow> = sqlx::query_as(&format!(
            "{} GROUP BY a.id HAVING COUNT(t.id) > 0 ORDER BY a.name",
            album_select_visible(&pred)
        ))
        .fetch_all(self.pool)
        .await?;
        Ok(rows.into_iter().map(Album::from).collect())
    }

    /// 按主键取专辑 DTO，仅当其含至少一条 `viewer` 可见曲目。
    pub async fn get_album_visible(&self, viewer: &Viewer, id: i64) -> Result<Option<Album>> {
        let pred = self.visibility(viewer, "t");
        let row: Option<AlbumRow> = sqlx::query_as(&format!(
            "{} WHERE a.id = ? GROUP BY a.id HAVING COUNT(t.id) > 0",
            album_select_visible(&pred)
        ))
        .bind(id)
        .fetch_optional(self.pool)
        .await?;
        Ok(row.map(Album::from))
    }

    /// 列举某艺人下 `viewer` 可见的专辑。
    pub async fn artist_albums_visible(
        &self,
        viewer: &Viewer,
        artist_id: i64,
    ) -> Result<Vec<Album>> {
        let pred = self.visibility(viewer, "t");
        let rows: Vec<AlbumRow> = sqlx::query_as(&format!(
            "{} WHERE a.artist_id = ? GROUP BY a.id HAVING COUNT(t.id) > 0 ORDER BY a.year, a.name",
            album_select_visible(&pred)
        ))
        .bind(artist_id)
        .fetch_all(self.pool)
        .await?;
        Ok(rows.into_iter().map(Album::from).collect())
    }

    /// 列举 `viewer` 可见的艺人（含至少一条可见曲目；专辑数只计有可见曲目的专辑）。
    pub async fn list_artists_visible(&self, viewer: &Viewer) -> Result<Vec<Artist>> {
        let pred = self.visibility(viewer, "tv");
        let rows: Vec<ArtistRow> = sqlx::query_as(&format!(
            "{} ORDER BY COALESCE(ar.sort_name, ar.name)",
            artist_select_visible(&pred)
        ))
        .fetch_all(self.pool)
        .await?;
        Ok(rows.into_iter().map(Artist::from).collect())
    }

    /// 按主键取艺人 DTO，仅当其含至少一条 `viewer` 可见曲目。
    pub async fn get_artist_visible(&self, viewer: &Viewer, id: i64) -> Result<Option<Artist>> {
        let pred = self.visibility(viewer, "tv");
        let row: Option<ArtistRow> =
            sqlx::query_as(&format!("{} AND ar.id = ?", artist_select_visible(&pred)))
                .bind(id)
                .fetch_optional(self.pool)
                .await?;
        Ok(row.map(Artist::from))
    }

    /// FTS5 搜索，仅返回 `viewer` 可见的命中（曲目直判；专辑/艺人按是否含可见曲目）。
    pub async fn search_visible(
        &self,
        viewer: &Viewer,
        query: &str,
        limit: i64,
    ) -> Result<SearchResults> {
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
                    if let Some(a) = self.get_artist_visible(viewer, ref_id).await? {
                        results.artists.push(a);
                    }
                }
                "album" => {
                    if let Some(a) = self.get_album_visible(viewer, ref_id).await? {
                        results.albums.push(a);
                    }
                }
                "track" => {
                    if let Some(t) = self.get_track_visible(viewer, ref_id).await? {
                        results.tracks.push(t);
                    }
                }
                _ => {}
            }
        }
        Ok(results)
    }
}

/// 专辑 SELECT，但曲目 JOIN 附带可见性谓词：聚合计数只计可见曲目。
/// 配合 `HAVING COUNT(t.id) > 0` 隐藏无可见曲目的专辑。
fn album_select_visible(pred: &str) -> String {
    format!(
        "SELECT a.id, a.name, a.artist_id, ar.name AS artist_name, a.year, a.genre, \
                a.cover_key, a.added_at, \
                COUNT(t.id) AS song_count, COALESCE(SUM(t.duration), 0) AS duration \
         FROM albums a \
         LEFT JOIN artists ar ON a.artist_id = ar.id \
         LEFT JOIN tracks t ON t.album_id = a.id AND ({pred})"
    )
}

/// 艺人 SELECT，仅保留含至少一条可见曲目的艺人；专辑数只计有可见曲目的专辑。
/// 谓词作用于内层曲目别名 `tv`。返回串以 `WHERE ... ` 结尾便于追加条件。
fn artist_select_visible(pred: &str) -> String {
    format!(
        "SELECT ar.id, ar.name, ar.sort_name, ar.mbid, ar.cover_key, \
                (SELECT COUNT(*) FROM albums al WHERE al.artist_id = ar.id \
                   AND EXISTS(SELECT 1 FROM tracks tv WHERE tv.album_id = al.id AND ({pred}))) \
                AS album_count \
         FROM artists ar \
         WHERE EXISTS(SELECT 1 FROM tracks tv WHERE tv.artist_id = ar.id AND ({pred}))"
    )
}
