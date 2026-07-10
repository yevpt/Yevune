//! 增量比对与索引写入：载入 `tracks` 现状、写入曲目/专辑/艺人、维护 `scan_state` 断点。
//!
//! 只写 `tracks.object_key` 前缀命中的行，因此按前缀扫描时删除判定仅作用于该前缀范围内。

use std::collections::HashMap;

use sqlx::SqlitePool;

use crate::index::{Index, NewTrack};

use super::tags::ParsedTrack;
use super::Result;
use crate::storage::ListEntry;

/// DB 现有曲目：`object_key -> (id, etag)`。
pub type ExistingTracks = HashMap<String, (i64, Option<String>)>;

/// 载入 `object_key` 以 `prefix` 开头的现有曲目及其 etag。
pub async fn existing_tracks(pool: &SqlitePool, prefix: &str) -> Result<ExistingTracks> {
    let pattern = format!("{}%", escape_like(prefix));
    let rows: Vec<(i64, String, Option<String>)> = sqlx::query_as(
        "SELECT id, object_key, etag FROM tracks WHERE object_key LIKE ?1 ESCAPE '\\'",
    )
    .bind(pattern)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, key, etag)| (key, (id, etag)))
        .collect())
}

/// upsert 一条曲目（含其艺人/专辑），并把封面记到专辑（仅当专辑尚无封面时）。
pub async fn upsert_track(
    index: &Index,
    entry: &ListEntry,
    meta: &ParsedTrack,
    cover_key: Option<&str>,
) -> Result<()> {
    let media = index.media();

    let artist_id = match meta.artist.as_deref() {
        Some(name) if !name.is_empty() => Some(media.upsert_artist(name).await?),
        _ => None,
    };
    let album_id = match meta.album.as_deref() {
        Some(name) if !name.is_empty() => Some(
            media
                .upsert_album(name, artist_id, meta.year, meta.genre.as_deref())
                .await?,
        ),
        _ => None,
    };

    if let (Some(aid), Some(ck)) = (album_id, cover_key) {
        set_album_cover(index.pool(), aid, ck).await?;
    }

    let new = NewTrack {
        title: meta
            .title
            .clone()
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| fallback_title(&entry.key)),
        album_id,
        artist_id,
        disc_no: meta.disc_no,
        track_no: meta.track_no,
        year: meta.year,
        genre: meta.genre.clone(),
        duration: meta.duration_secs,
        codec: meta.codec.clone(),
        bitrate: compute_bitrate(entry.size, meta.duration_secs),
        size: Some(entry.size),
        object_key: entry.key.clone(),
        etag: entry.etag.clone(),
        content_hash: None,
        replaygain: None,
    };
    media.upsert_track(&new).await?;
    Ok(())
}

/// 仅当专辑还没有封面时写入 `cover_key`（避免同专辑多曲反复改写）。
async fn set_album_cover(pool: &SqlitePool, album_id: i64, cover_key: &str) -> Result<()> {
    sqlx::query("UPDATE albums SET cover_key = ? WHERE id = ? AND cover_key IS NULL")
        .bind(cover_key)
        .bind(album_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// 持久化列举断点游标（`None` 表示已到末页/清空）。
///
/// 目前作为进度/断点标记落库；从游标真正续扫（跳过已扫前缀 + 延后删除判定）留作后续增强。
pub async fn set_cursor(pool: &SqlitePool, cursor: Option<&str>) -> Result<()> {
    sqlx::query("UPDATE scan_state SET cursor = ? WHERE id = 1")
        .bind(cursor)
        .execute(pool)
        .await?;
    Ok(())
}

/// 标记扫描完成：清空游标并记录完成时间。
pub async fn finish_scan(pool: &SqlitePool) -> Result<()> {
    sqlx::query("UPDATE scan_state SET cursor = NULL, last_scan_at = datetime('now') WHERE id = 1")
        .execute(pool)
        .await?;
    Ok(())
}

/// 读取上次扫描完成时间。
pub async fn last_scan_at(pool: &SqlitePool) -> Result<Option<String>> {
    let row: (Option<String>,) = sqlx::query_as("SELECT last_scan_at FROM scan_state WHERE id = 1")
        .fetch_one(pool)
        .await?;
    Ok(row.0)
}

/// 由大小与时长估算码率（kbps）；缺时长则未知。
fn compute_bitrate(size: u64, duration_secs: Option<u32>) -> Option<u32> {
    match duration_secs {
        Some(d) if d > 0 => Some((size * 8 / (d as u64 * 1000)) as u32),
        _ => None,
    }
}

/// 无标题标签时以文件名（去扩展名）兜底。
fn fallback_title(key: &str) -> String {
    let base = key.rsplit('/').next().unwrap_or(key);
    base.rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(base)
        .to_string()
}

/// 转义 LIKE 通配符（`\` `%` `_`），配合 `ESCAPE '\'` 使前缀按字面匹配。
fn escape_like(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}
