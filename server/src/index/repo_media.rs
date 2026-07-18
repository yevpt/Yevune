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
            starred: None,
            user_rating: None,
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
    object_key: String,
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
            path: Some(r.object_key),
            starred: None,
            user_rating: None,
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
            starred: None,
            user_rating: None,
        }
    }
}

/// 取单曲目的 SELECT（含 LEFT JOIN 专辑/艺人）。
const TRACK_SELECT: &str = "\
SELECT t.id, COALESCE(title_override.value, t.title) AS title, \
       t.album_id, CASE WHEN album_override.track_id IS NULL THEN a.name ELSE album_override.value END AS album_name, \
       t.artist_id, CASE WHEN artist_override.track_id IS NULL THEN ar.name ELSE artist_override.value END AS artist_name, \
       CASE WHEN disc_override.track_id IS NULL THEN t.disc_no ELSE CAST(disc_override.value AS INTEGER) END AS disc_no, \
       CASE WHEN track_override.track_id IS NULL THEN t.track_no ELSE CAST(track_override.value AS INTEGER) END AS track_no, \
       CASE WHEN year_override.track_id IS NULL THEN t.year ELSE CAST(year_override.value AS INTEGER) END AS year, \
       CASE WHEN genre_override.track_id IS NULL THEN t.genre ELSE genre_override.value END AS genre, \
       t.duration, t.codec, t.bitrate, t.size, a.cover_key AS cover_key, t.added_at, t.object_key \
FROM tracks t \
LEFT JOIN albums a ON t.album_id = a.id \
LEFT JOIN artists ar ON t.artist_id = ar.id \
LEFT JOIN tag_overrides title_override ON title_override.track_id=t.id AND title_override.field='title' \
LEFT JOIN tag_overrides album_override ON album_override.track_id=t.id AND album_override.field='album' \
LEFT JOIN tag_overrides artist_override ON artist_override.track_id=t.id AND artist_override.field='artist' \
LEFT JOIN tag_overrides disc_override ON disc_override.track_id=t.id AND disc_override.field='discNumber' \
LEFT JOIN tag_overrides track_override ON track_override.track_id=t.id AND track_override.field='track' \
LEFT JOIN tag_overrides year_override ON year_override.track_id=t.id AND year_override.field='year' \
LEFT JOIN tag_overrides genre_override ON genre_override.track_id=t.id AND genre_override.field='genre'";

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

/// `search3` 各实体类型独立的数据库分页窗口。
#[derive(Debug, Clone, Copy)]
pub struct SearchPage {
    /// 跳过的艺人结果数。
    pub artist_offset: i64,
    /// 返回的艺人结果上限。
    pub artist_count: i64,
    /// 跳过的专辑结果数。
    pub album_offset: i64,
    /// 返回的专辑结果上限。
    pub album_count: i64,
    /// 跳过的曲目结果数。
    pub track_offset: i64,
    /// 返回的曲目结果上限。
    pub track_count: i64,
}

/// 媒体传输所需的内部存储定位信息（仅服务端使用）。
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct MediaSource {
    /// 曲目主键。
    pub id: i64,
    /// Garage 原始对象键。
    pub object_key: String,
    /// 当前原始对象 ETag（移动失败回滚时恢复索引）。
    pub etag: Option<String>,
    /// 原始编码/后缀。
    pub codec: Option<String>,
    /// 原始码率（kbps）。
    pub bitrate: Option<i64>,
    /// 原始大小。
    pub size: Option<i64>,
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

    /// 列举全部艺人（按排序名/展示名排序）。
    pub async fn list_artists(&self) -> Result<Vec<Artist>> {
        let rows: Vec<ArtistRow> = sqlx::query_as(&format!(
            "{ARTIST_SELECT} GROUP BY ar.id ORDER BY COALESCE(ar.sort_name, ar.name), ar.name"
        ))
        .fetch_all(self.pool)
        .await?;
        Ok(rows.into_iter().map(Artist::from).collect())
    }

    /// 分页列举艺人，避免空 `search3` 整库加载。
    pub async fn list_artists_page(&self, offset: i64, limit: i64) -> Result<Vec<Artist>> {
        let rows: Vec<ArtistRow> = sqlx::query_as(&format!(
            "{ARTIST_SELECT} GROUP BY ar.id ORDER BY COALESCE(ar.sort_name, ar.name), ar.name LIMIT ? OFFSET ?"
        ))
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool)
        .await?;
        Ok(rows.into_iter().map(Artist::from).collect())
    }

    /// 列举某艺人的全部专辑。
    pub async fn albums_by_artist(&self, artist_id: i64) -> Result<Vec<Album>> {
        let rows: Vec<AlbumRow> = sqlx::query_as(&format!(
            "{ALBUM_SELECT} WHERE a.artist_id = ? GROUP BY a.id ORDER BY a.name"
        ))
        .bind(artist_id)
        .fetch_all(self.pool)
        .await?;
        Ok(rows.into_iter().map(Album::from).collect())
    }

    /// 列举某专辑的曲目，按碟号、曲号与标题排序。
    pub async fn tracks_by_album(&self, album_id: i64) -> Result<Vec<Track>> {
        let rows: Vec<TrackRow> = sqlx::query_as(&format!(
            "{TRACK_SELECT} WHERE t.album_id = ? ORDER BY \
             COALESCE(CASE WHEN disc_override.track_id IS NULL THEN t.disc_no ELSE CAST(disc_override.value AS INTEGER) END, 0), \
             COALESCE(CASE WHEN track_override.track_id IS NULL THEN t.track_no ELSE CAST(track_override.value AS INTEGER) END, 0), \
             COALESCE(title_override.value, t.title)"
        ))
        .bind(album_id)
        .fetch_all(self.pool)
        .await?;
        Ok(rows.into_iter().map(Track::from).collect())
    }

    /// 列举全部曲目，供空 `search3` 的离线同步语义使用。
    pub async fn list_tracks(&self) -> Result<Vec<Track>> {
        let rows: Vec<TrackRow> = sqlx::query_as(&format!(
            "{TRACK_SELECT} ORDER BY COALESCE((SELECT value FROM tag_overrides o \
             WHERE o.track_id=t.id AND o.field='title'), t.title)"
        ))
        .fetch_all(self.pool)
        .await?;
        Ok(rows.into_iter().map(Track::from).collect())
    }

    /// 分页列举专辑，供空搜索使用。
    pub async fn list_albums_page(&self, offset: i64, limit: i64) -> Result<Vec<Album>> {
        let rows: Vec<AlbumRow> = sqlx::query_as(&format!(
            "{ALBUM_SELECT} GROUP BY a.id ORDER BY a.name LIMIT ? OFFSET ?"
        ))
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool)
        .await?;
        Ok(rows.into_iter().map(Album::from).collect())
    }

    /// 分页列举曲目，供空搜索使用。
    pub async fn list_tracks_page(&self, offset: i64, limit: i64) -> Result<Vec<Track>> {
        let rows: Vec<TrackRow> = sqlx::query_as(&format!(
            "{TRACK_SELECT} ORDER BY COALESCE((SELECT value FROM tag_overrides o \
                 WHERE o.track_id=t.id AND o.field='title'), t.title) LIMIT ? OFFSET ?"
        ))
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool)
        .await?;
        Ok(rows.into_iter().map(Track::from).collect())
    }

    /// 封面标识是否由媒体索引签发，防止把任意对象键当封面读取。
    pub async fn has_cover_key(&self, key: &str) -> Result<bool> {
        let count: i64 = sqlx::query_scalar(
            "SELECT (SELECT COUNT(*) FROM albums WHERE cover_key = ?) + \
                    (SELECT COUNT(*) FROM artists WHERE cover_key = ?)",
        )
        .bind(key)
        .bind(key)
        .fetch_one(self.pool)
        .await?;
        Ok(count > 0)
    }

    /// 替换专辑封面对象键；专辑不存在时返回 `false`。
    pub async fn set_album_cover(&self, album_id: i64, key: &str) -> Result<bool> {
        let result = sqlx::query("UPDATE albums SET cover_key = ? WHERE id = ?")
            .bind(key)
            .bind(album_id)
            .execute(self.pool)
            .await?;
        Ok(result.rows_affected() == 1)
    }

    /// 在数据库侧先按 `viewer` 可见性过滤，再完成 `getAlbumList2` 排序与分页，仅返回本页主键。
    #[allow(clippy::too_many_arguments)]
    pub async fn album_ids_for_list(
        &self,
        viewer: &Viewer,
        kind: &str,
        offset: i64,
        limit: i64,
        from_year: Option<u32>,
        to_year: Option<u32>,
        genre: Option<&str>,
    ) -> Result<Vec<i64>> {
        let track_visible = self.visibility(viewer, "vt");
        let album_visible = format!(
            "(EXISTS(SELECT 1 FROM tracks vt WHERE vt.album_id = a.id AND ({track_visible})) \
             OR NOT EXISTS(SELECT 1 FROM tracks all_t WHERE all_t.album_id = a.id))"
        );
        match kind {
            "random" => {
                let sql = format!(
                    "SELECT a.id FROM albums a WHERE {album_visible} \
                     ORDER BY random() LIMIT ? OFFSET ?"
                );
                sqlx::query_scalar(&sql)
                    .bind(limit)
                    .bind(offset)
                    .fetch_all(self.pool)
                    .await
            }
            "newest" => {
                let sql = format!(
                    "SELECT a.id FROM albums a WHERE {album_visible} \
                     ORDER BY a.added_at DESC, a.id DESC LIMIT ? OFFSET ?"
                );
                sqlx::query_scalar(&sql)
                    .bind(limit)
                    .bind(offset)
                    .fetch_all(self.pool)
                    .await
            }
            "alphabeticalByArtist" => {
                let sql = format!(
                    "SELECT a.id FROM albums a LEFT JOIN artists ar ON ar.id = a.artist_id \
                     WHERE {album_visible} ORDER BY COALESCE(ar.sort_name, ar.name), \
                     a.name, a.id ASC LIMIT ? OFFSET ?"
                );
                sqlx::query_scalar(&sql)
                    .bind(limit)
                    .bind(offset)
                    .fetch_all(self.pool)
                    .await
            }
            "byYear" => {
                let from = from_year.unwrap_or(0);
                let to = to_year.unwrap_or(0);
                let order = if from > to { "DESC" } else { "ASC" };
                let sql = format!(
                    "SELECT a.id FROM albums a WHERE a.year BETWEEN ? AND ? \
                     AND {album_visible} ORDER BY a.year {order}, a.name, a.id ASC LIMIT ? OFFSET ?"
                );
                sqlx::query_scalar(&sql)
                    .bind(from.min(to))
                    .bind(from.max(to))
                    .bind(limit)
                    .bind(offset)
                    .fetch_all(self.pool)
                    .await
            }
            "byGenre" => {
                let sql = format!(
                    "SELECT a.id FROM albums a WHERE a.genre = ? AND {album_visible} \
                     ORDER BY a.name, a.id ASC LIMIT ? OFFSET ?"
                );
                sqlx::query_scalar(&sql)
                    .bind(genre)
                    .bind(limit)
                    .bind(offset)
                    .fetch_all(self.pool)
                    .await
            }
            "highest" => {
                let sql = format!(
                    "SELECT a.id FROM albums a JOIN annotations n \
                     ON n.user_id = ? AND n.item_type = 'album' AND n.item_id = a.id \
                     WHERE n.rating IS NOT NULL AND {album_visible} \
                     ORDER BY n.rating DESC, a.name, a.id ASC LIMIT ? OFFSET ?"
                );
                sqlx::query_scalar(&sql)
                    .bind(viewer.user_id)
                    .bind(limit)
                    .bind(offset)
                    .fetch_all(self.pool)
                    .await
            }
            "frequent" => {
                let visible_track = self.visibility(viewer, "t");
                let sql = format!(
                    "SELECT a.id FROM albums a JOIN tracks t ON t.album_id = a.id \
                 JOIN annotations n ON n.user_id = ? AND n.item_type = 'track' AND n.item_id = t.id \
                     WHERE ({visible_track}) GROUP BY a.id HAVING SUM(n.play_count) > 0 \
                     ORDER BY SUM(n.play_count) DESC, a.name, a.id ASC LIMIT ? OFFSET ?"
                );
                sqlx::query_scalar(&sql)
                    .bind(viewer.user_id)
                    .bind(limit)
                    .bind(offset)
                    .fetch_all(self.pool)
                    .await
            }
            "recent" => {
                let visible_track = self.visibility(viewer, "t");
                let sql = format!(
                    "SELECT a.id FROM albums a JOIN tracks t ON t.album_id = a.id \
                 JOIN annotations n ON n.user_id = ? AND n.item_type = 'track' AND n.item_id = t.id \
                     WHERE n.last_played IS NOT NULL AND ({visible_track}) GROUP BY a.id \
                     ORDER BY MAX(n.last_played) DESC, a.name, a.id ASC LIMIT ? OFFSET ?"
                );
                sqlx::query_scalar(&sql)
                    .bind(viewer.user_id)
                    .bind(limit)
                    .bind(offset)
                    .fetch_all(self.pool)
                    .await
            }
            "starred" => {
                let sql = format!(
                    "SELECT a.id FROM albums a JOIN annotations n \
                 ON n.user_id = ? AND n.item_type = 'album' AND n.item_id = a.id \
                     WHERE n.starred_at IS NOT NULL AND {album_visible} \
                     ORDER BY n.starred_at DESC, a.id DESC LIMIT ? OFFSET ?"
                );
                sqlx::query_scalar(&sql)
                    .bind(viewer.user_id)
                    .bind(limit)
                    .bind(offset)
                    .fetch_all(self.pool)
                    .await
            }
            _ => {
                let sql = format!(
                    "SELECT a.id FROM albums a WHERE {album_visible} \
                     ORDER BY a.name, a.id ASC LIMIT ? OFFSET ?"
                );
                sqlx::query_scalar(&sql)
                    .bind(limit)
                    .bind(offset)
                    .fetch_all(self.pool)
                    .await
            }
        }
    }

    /// 聚合非空流派的曲目数与专辑数。
    pub async fn list_genres(&self) -> Result<Vec<contract::Genre>> {
        let rows: Vec<(String, i64, i64)> = sqlx::query_as(
            "SELECT CASE WHEN o.track_id IS NULL THEN t.genre ELSE o.value END AS display_genre, \
                    COUNT(DISTINCT t.id), COUNT(DISTINCT t.album_id) \
             FROM tracks t \
             LEFT JOIN tag_overrides o ON o.track_id = t.id AND o.field = 'genre' \
             WHERE (CASE WHEN o.track_id IS NULL THEN t.genre ELSE o.value END) IS NOT NULL \
               AND (CASE WHEN o.track_id IS NULL THEN t.genre ELSE o.value END) <> '' \
             GROUP BY display_genre ORDER BY display_genre",
        )
        .fetch_all(self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(value, songs, albums)| contract::Genre {
                value,
                song_count: songs as u32,
                album_count: albums as u32,
            })
            .collect())
    }

    /// 聚合流派，但只计 `viewer` 可见的曲目：仅含至少一条可见曲目的流派，计数只计可见曲目。
    pub async fn list_genres_visible(&self, viewer: &Viewer) -> Result<Vec<contract::Genre>> {
        let pred = self.visibility(viewer, "t");
        let rows: Vec<(String, i64, i64)> = sqlx::query_as(&format!(
            "SELECT CASE WHEN o.track_id IS NULL THEN t.genre ELSE o.value END AS display_genre, \
                    COUNT(DISTINCT t.id), COUNT(DISTINCT t.album_id) \
             FROM tracks t \
             LEFT JOIN tag_overrides o ON o.track_id = t.id AND o.field = 'genre' \
             WHERE (CASE WHEN o.track_id IS NULL THEN t.genre ELSE o.value END) IS NOT NULL \
               AND (CASE WHEN o.track_id IS NULL THEN t.genre ELSE o.value END) <> '' \
               AND ({pred}) \
             GROUP BY display_genre ORDER BY display_genre"
        ))
        .fetch_all(self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(value, songs, albums)| contract::Genre {
                value,
                song_count: songs as u32,
                album_count: albums as u32,
            })
            .collect())
    }

    /// 读取曲目原始对象定位信息，供 stream/download 使用。
    pub async fn media_source(&self, id: i64) -> Result<Option<MediaSource>> {
        sqlx::query_as("SELECT id, object_key, etag, codec, bitrate, size FROM tracks WHERE id = ?")
            .bind(id)
            .fetch_optional(self.pool)
            .await
    }

    /// 仅当旧对象键与 ETag 仍匹配时，原子更新对象定位信息。
    pub async fn move_source_cas(
        &self,
        id: i64,
        old_object_key: &str,
        old_etag: Option<&str>,
        object_key: &str,
        etag: Option<&str>,
        size: u64,
    ) -> Result<bool> {
        let affected = sqlx::query(
            "UPDATE tracks SET object_key = ?, etag = ?, size = ? \
             WHERE id = ? AND object_key = ? AND etag IS ?",
        )
        .bind(object_key)
        .bind(etag)
        .bind(size as i64)
        .bind(id)
        .bind(old_object_key)
        .bind(old_etag)
        .execute(self.pool)
        .await?
        .rows_affected();
        Ok(affected > 0)
    }

    /// upsert 指定曲目的标签覆盖值；未提供的字段保持不变。
    pub async fn set_tag_overrides(
        &self,
        id: i64,
        values: &[(&str, Option<&str>)],
    ) -> Result<bool> {
        let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tracks WHERE id = ?")
            .bind(id)
            .fetch_one(self.pool)
            .await?;
        if exists == 0 {
            return Ok(false);
        }
        let mut tx = self.pool.begin().await?;
        for (field, value) in values {
            sqlx::query(
                "INSERT INTO tag_overrides(track_id, field, value) VALUES(?, ?, ?) \
                 ON CONFLICT(track_id, field) DO UPDATE SET value = excluded.value",
            )
            .bind(id)
            .bind(field)
            .bind(value)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(true)
    }

    /// FTS5 搜索艺人/专辑/曲目名，按类型分组返回。
    pub async fn search(&self, query: &str, limit: i64) -> Result<SearchResults> {
        self.search_page(
            query,
            SearchPage {
                artist_offset: 0,
                artist_count: limit,
                album_offset: 0,
                album_count: limit,
                track_offset: 0,
                track_count: limit,
            },
        )
        .await
    }

    /// FTS5 搜索并在数据库侧对各实体类型独立分页。
    pub async fn search_page(&self, query: &str, page: SearchPage) -> Result<SearchResults> {
        // 把用户输入包装成 FTS5 字面短语，避免 `/`、`-`、引号等被解释为查询语法。
        let literal = format!("\"{}\"", query.replace('"', "\"\""));
        let mut results = SearchResults::default();
        for id in self
            .search_ids(&literal, "artist", page.artist_offset, page.artist_count)
            .await?
        {
            if let Some(artist) = self.get_artist(id).await? {
                results.artists.push(artist);
            }
        }
        for id in self
            .search_ids(&literal, "album", page.album_offset, page.album_count)
            .await?
        {
            if let Some(album) = self.get_album(id).await? {
                results.albums.push(album);
            }
        }
        for id in self
            .search_track_ids(query, page.track_offset, page.track_count)
            .await?
        {
            if let Some(track) = self.get_track(id).await? {
                results.tracks.push(track);
            }
        }
        Ok(results)
    }

    async fn search_ids(
        &self,
        literal: &str,
        kind: &str,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<i64>> {
        sqlx::query_scalar(
            "SELECT ref_id FROM search_fts \
             WHERE search_fts MATCH ? AND kind = ? ORDER BY rowid LIMIT ? OFFSET ?",
        )
        .bind(literal)
        .bind(kind)
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool)
        .await
    }

    async fn search_track_ids(&self, query: &str, offset: i64, limit: i64) -> Result<Vec<i64>> {
        sqlx::query_scalar(
            "SELECT t.id FROM tracks t \
             LEFT JOIN tag_overrides o ON o.track_id=t.id AND o.field='title' \
             WHERE instr(lower(COALESCE(o.value, t.title)), lower(?)) > 0 \
             ORDER BY lower(COALESCE(o.value, t.title)), t.id LIMIT ? OFFSET ?",
        )
        .bind(query)
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool)
        .await
    }

    /// 在事务内清除一组已经显式写回原文件的覆盖字段。
    pub async fn clear_tag_overrides(&self, id: i64, fields: &[&str]) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        for field in fields {
            sqlx::query("DELETE FROM tag_overrides WHERE track_id = ? AND field = ?")
                .bind(id)
                .bind(field)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await
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

    /// 列举某专辑内 `viewer` 可见的曲目（按碟/曲序；排序尊重标签覆盖层）。
    pub async fn album_tracks_visible(&self, viewer: &Viewer, album_id: i64) -> Result<Vec<Track>> {
        let pred = self.visibility(viewer, "t");
        let rows: Vec<TrackRow> = sqlx::query_as(&format!(
            "{TRACK_SELECT} WHERE t.album_id = ? AND ({pred}) ORDER BY \
             COALESCE(CASE WHEN disc_override.track_id IS NULL THEN t.disc_no ELSE CAST(disc_override.value AS INTEGER) END, 0), \
             COALESCE(CASE WHEN track_override.track_id IS NULL THEN t.track_no ELSE CAST(track_override.value AS INTEGER) END, 0), \
             COALESCE(title_override.value, t.title)"
        ))
        .bind(album_id)
        .fetch_all(self.pool)
        .await?;
        Ok(rows.into_iter().map(Track::from).collect())
    }

    /// 列举 `viewer` 可见的专辑（含至少一条可见曲目，或本就无曲目；计数只计可见曲目）。
    pub async fn list_albums_visible(&self, viewer: &Viewer) -> Result<Vec<Album>> {
        let pred = self.visibility(viewer, "t");
        let rows: Vec<AlbumRow> = sqlx::query_as(&format!(
            "{} GROUP BY a.id HAVING {} ORDER BY a.name",
            album_select_visible(&pred),
            ALBUM_VISIBLE_HAVING
        ))
        .fetch_all(self.pool)
        .await?;
        Ok(rows.into_iter().map(Album::from).collect())
    }

    /// 按主键取专辑 DTO，仅当其含至少一条 `viewer` 可见曲目（或本就无曲目）。
    pub async fn get_album_visible(&self, viewer: &Viewer, id: i64) -> Result<Option<Album>> {
        let pred = self.visibility(viewer, "t");
        let row: Option<AlbumRow> = sqlx::query_as(&format!(
            "{} WHERE a.id = ? GROUP BY a.id HAVING {}",
            album_select_visible(&pred),
            ALBUM_VISIBLE_HAVING
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
            "{} WHERE a.artist_id = ? GROUP BY a.id HAVING {} ORDER BY a.year, a.name",
            album_select_visible(&pred),
            ALBUM_VISIBLE_HAVING
        ))
        .bind(artist_id)
        .fetch_all(self.pool)
        .await?;
        Ok(rows.into_iter().map(Album::from).collect())
    }

    /// 列举 `viewer` 可见的曲目（按标题排序），供空 `search3` 浏览语义使用。
    pub async fn list_tracks_visible(&self, viewer: &Viewer) -> Result<Vec<Track>> {
        let pred = self.visibility(viewer, "t");
        let rows: Vec<TrackRow> = sqlx::query_as(&format!(
            "{TRACK_SELECT} WHERE ({pred}) ORDER BY COALESCE((SELECT value FROM tag_overrides o \
             WHERE o.track_id=t.id AND o.field='title'), t.title), t.id"
        ))
        .fetch_all(self.pool)
        .await?;
        Ok(rows.into_iter().map(Track::from).collect())
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

    /// 取曲目的原始对象键与编码（供 stream/download 透传，不进入客户端 DTO）。
    pub async fn track_source(&self, id: i64) -> Result<Option<(String, Option<String>)>> {
        let row: Option<(String, Option<String>)> =
            sqlx::query_as("SELECT object_key, codec FROM tracks WHERE id = ?")
                .bind(id)
                .fetch_optional(self.pool)
                .await?;
        Ok(row)
    }

    /// 某封面键是否对 `viewer` 可见：存在引用它且含可见曲目的专辑，或引用它的艺人有可见曲目。
    pub async fn cover_key_visible(&self, viewer: &Viewer, cover_key: &str) -> Result<bool> {
        let pred = self.visibility(viewer, "t");
        let sql = format!(
            "SELECT EXISTS(\
                 SELECT 1 FROM albums a JOIN tracks t ON t.album_id = a.id \
                 WHERE a.cover_key = ? AND ({pred})\
             ) OR EXISTS(\
                 SELECT 1 FROM artists ar JOIN tracks t ON t.artist_id = ar.id \
                 WHERE ar.cover_key = ? AND ({pred})\
             )"
        );
        let visible: i64 = sqlx::query_scalar(&sql)
            .bind(cover_key)
            .bind(cover_key)
            .fetch_one(self.pool)
            .await?;
        Ok(visible != 0)
    }

    /// 搜索并仅返回 `viewer` 可见的命中。
    ///
    /// 匹配方式与 [`search_page`](Self::search_page) 一致（艺人/专辑走 FTS5，曲目按
    /// 覆盖层标题 `instr` 匹配），从而尊重标签覆盖；再逐条经可见性过滤。
    pub async fn search_visible(
        &self,
        viewer: &Viewer,
        query: &str,
        limit: i64,
    ) -> Result<SearchResults> {
        let literal = format!("\"{}\"", query.replace('"', "\"\""));
        let mut results = SearchResults::default();
        for id in self.search_ids(&literal, "artist", 0, limit).await? {
            if let Some(artist) = self.get_artist_visible(viewer, id).await? {
                results.artists.push(artist);
            }
        }
        for id in self.search_ids(&literal, "album", 0, limit).await? {
            if let Some(album) = self.get_album_visible(viewer, id).await? {
                results.albums.push(album);
            }
        }
        for id in self.search_track_ids(query, 0, limit).await? {
            if let Some(track) = self.get_track_visible(viewer, id).await? {
                results.tracks.push(track);
            }
        }
        Ok(results)
    }
}

/// 专辑可见性 `HAVING` 条件：含至少一条可见曲目，或本就无任何曲目（无内容可限制→默认开放）。
/// 只隐藏"有曲目但对当前访问者全部受限"的专辑。
const ALBUM_VISIBLE_HAVING: &str =
    "(COUNT(t.id) > 0 OR (SELECT COUNT(*) FROM tracks tt WHERE tt.album_id = a.id) = 0)";

/// 专辑 SELECT，但曲目 JOIN 附带可见性谓词：聚合计数只计可见曲目。
/// 配合 [`ALBUM_VISIBLE_HAVING`] 隐藏"有曲目但全部受限"的专辑。
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

/// 艺人 SELECT，保留含至少一条可见曲目的艺人，或本就无曲目的艺人（无内容可限制→默认开放）；
/// 专辑数只计有可见曲目的专辑。谓词作用于内层曲目别名 `tv`。返回串以 `WHERE (...)` 结尾便于追加条件。
fn artist_select_visible(pred: &str) -> String {
    format!(
        "SELECT ar.id, ar.name, ar.sort_name, ar.mbid, ar.cover_key, \
                (SELECT COUNT(*) FROM albums al WHERE al.artist_id = ar.id \
                   AND EXISTS(SELECT 1 FROM tracks tv WHERE tv.album_id = al.id AND ({pred}))) \
                AS album_count \
         FROM artists ar \
         WHERE (EXISTS(SELECT 1 FROM tracks tv WHERE tv.artist_id = ar.id AND ({pred})) \
                OR NOT EXISTS(SELECT 1 FROM tracks tt WHERE tt.artist_id = ar.id))"
    )
}
