//! 歌单与文件夹树仓储（每用户独立树，`owner_id` 隔离）。

use contract::{Playlist, PlaylistFolder};
use sqlx::{FromRow, SqlitePool};

use super::Result;

#[derive(FromRow)]
struct FolderRow {
    id: i64,
    owner_id: i64,
    name: String,
    parent_id: Option<i64>,
    position: i64,
}

impl From<FolderRow> for PlaylistFolder {
    fn from(r: FolderRow) -> Self {
        PlaylistFolder {
            id: r.id.to_string(),
            owner_id: r.owner_id.to_string(),
            name: r.name,
            parent_id: r.parent_id.map(|i| i.to_string()),
            position: r.position as u32,
        }
    }
}

#[derive(FromRow)]
struct PlaylistRow {
    id: i64,
    owner_id: i64,
    name: String,
    comment: Option<String>,
    folder_id: Option<i64>,
    position: i64,
    song_count: i64,
    duration: i64,
    created_at: String,
    changed_at: String,
}

impl From<PlaylistRow> for Playlist {
    fn from(r: PlaylistRow) -> Self {
        Playlist {
            id: r.id.to_string(),
            owner_id: r.owner_id.to_string(),
            name: r.name,
            comment: r.comment,
            folder_id: r.folder_id.map(|i| i.to_string()),
            position: r.position as u32,
            song_count: r.song_count as u32,
            duration: r.duration as u32,
            created: Some(sqlite_utc_to_rfc3339(r.created_at)),
            changed: Some(sqlite_utc_to_rfc3339(r.changed_at)),
        }
    }
}

fn sqlite_utc_to_rfc3339(mut value: String) -> String {
    if value.len() == 19 && value.as_bytes().get(10) == Some(&b' ') {
        value.replace_range(10..11, "T");
        value.push('Z');
    }
    value
}

const PLAYLIST_SELECT: &str = "\
SELECT p.id, p.owner_id, p.name, p.comment, p.folder_id, p.position, \
       COUNT(pt.track_id) AS song_count, COALESCE(SUM(t.duration), 0) AS duration, \
       p.created_at, p.changed_at \
FROM playlists p \
LEFT JOIN playlist_tracks pt ON pt.playlist_id = p.id \
LEFT JOIN tracks t ON t.id = pt.track_id";

/// 歌单/文件夹仓储。
pub struct PlaylistRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> PlaylistRepo<'a> {
    /// 绑定连接池。
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    // ── 文件夹 ──────────────────────────────────────────────
    /// 创建文件夹，返回主键。
    pub async fn create_folder(
        &self,
        owner_id: i64,
        name: &str,
        parent_id: Option<i64>,
    ) -> Result<i64> {
        sqlx::query_scalar(
            "INSERT INTO playlist_folders(owner_id, name, parent_id) VALUES(?, ?, ?) RETURNING id",
        )
        .bind(owner_id)
        .bind(name)
        .bind(parent_id)
        .fetch_one(self.pool)
        .await
    }

    /// 重命名文件夹，返回是否命中。
    pub async fn rename_folder(&self, id: i64, name: &str) -> Result<bool> {
        let affected = sqlx::query("UPDATE playlist_folders SET name = ? WHERE id = ?")
            .bind(name)
            .bind(id)
            .execute(self.pool)
            .await?
            .rows_affected();
        Ok(affected > 0)
    }

    /// 移动文件夹到新父级（`None` 为顶级），返回是否命中。
    pub async fn move_folder(&self, id: i64, new_parent: Option<i64>) -> Result<bool> {
        let affected = sqlx::query("UPDATE playlist_folders SET parent_id = ? WHERE id = ?")
            .bind(new_parent)
            .bind(id)
            .execute(self.pool)
            .await?
            .rows_affected();
        Ok(affected > 0)
    }

    /// 删除文件夹（级联删除子文件夹），返回是否命中。
    pub async fn delete_folder(&self, id: i64) -> Result<bool> {
        let affected = sqlx::query("DELETE FROM playlist_folders WHERE id = ?")
            .bind(id)
            .execute(self.pool)
            .await?
            .rows_affected();
        Ok(affected > 0)
    }

    /// 列举某用户的全部文件夹（扁平，含 parent_id 供构树）。
    pub async fn list_folders(&self, owner_id: i64) -> Result<Vec<PlaylistFolder>> {
        let rows: Vec<FolderRow> = sqlx::query_as(
            "SELECT id, owner_id, name, parent_id, position FROM playlist_folders \
             WHERE owner_id = ? ORDER BY position, name",
        )
        .bind(owner_id)
        .fetch_all(self.pool)
        .await?;
        Ok(rows.into_iter().map(PlaylistFolder::from).collect())
    }

    // ── 歌单 ────────────────────────────────────────────────
    /// 创建歌单，返回主键。
    pub async fn create_playlist(
        &self,
        owner_id: i64,
        name: &str,
        folder_id: Option<i64>,
    ) -> Result<i64> {
        sqlx::query_scalar(
            "INSERT INTO playlists(owner_id, name, folder_id) VALUES(?, ?, ?) RETURNING id",
        )
        .bind(owner_id)
        .bind(name)
        .bind(folder_id)
        .fetch_one(self.pool)
        .await
    }

    /// 在单一事务内创建歌单并写入有序曲目，任一外键失败时不留孤立歌单。
    pub async fn create_playlist_with_tracks(
        &self,
        owner_id: i64,
        name: &str,
        folder_id: Option<i64>,
        track_ids: &[i64],
    ) -> Result<i64> {
        let mut tx = self.pool.begin().await?;
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO playlists(owner_id, name, folder_id) VALUES(?, ?, ?) RETURNING id",
        )
        .bind(owner_id)
        .bind(name)
        .bind(folder_id)
        .fetch_one(&mut *tx)
        .await?;
        for (position, track_id) in track_ids.iter().enumerate() {
            sqlx::query(
                "INSERT INTO playlist_tracks(playlist_id, track_id, position) VALUES(?, ?, ?)",
            )
            .bind(id)
            .bind(track_id)
            .bind(position as i64)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(id)
    }

    /// 按主键取歌单 DTO（含曲目数与时长）。
    pub async fn get_playlist(&self, id: i64) -> Result<Option<Playlist>> {
        let row: Option<PlaylistRow> =
            sqlx::query_as(&format!("{PLAYLIST_SELECT} WHERE p.id = ? GROUP BY p.id"))
                .bind(id)
                .fetch_optional(self.pool)
                .await?;
        Ok(row.map(Playlist::from))
    }

    /// 更新歌单名与备注，返回是否命中。
    pub async fn update_playlist(
        &self,
        id: i64,
        name: &str,
        comment: Option<&str>,
    ) -> Result<bool> {
        let affected = sqlx::query(
            "UPDATE playlists SET name = ?, comment = ?, changed_at = datetime('now') WHERE id = ?",
        )
        .bind(name)
        .bind(comment)
        .bind(id)
        .execute(self.pool)
        .await?
        .rows_affected();
        Ok(affected > 0)
    }

    /// 在单一事务内更新歌单元数据与完整曲目顺序。
    pub async fn update_playlist_with_tracks(
        &self,
        id: i64,
        name: &str,
        comment: Option<&str>,
        track_ids: &[i64],
    ) -> Result<bool> {
        let mut tx = self.pool.begin().await?;
        let affected = sqlx::query(
            "UPDATE playlists SET name = ?, comment = ?, changed_at = datetime('now') WHERE id = ?",
        )
        .bind(name)
        .bind(comment)
        .bind(id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if affected == 0 {
            tx.rollback().await?;
            return Ok(false);
        }
        sqlx::query("DELETE FROM playlist_tracks WHERE playlist_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        for (position, track_id) in track_ids.iter().enumerate() {
            sqlx::query(
                "INSERT INTO playlist_tracks(playlist_id, track_id, position) VALUES(?, ?, ?)",
            )
            .bind(id)
            .bind(track_id)
            .bind(position as i64)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(true)
    }

    /// 移动歌单到新文件夹（`None` 为根级），返回是否命中。
    pub async fn move_playlist(&self, id: i64, new_folder: Option<i64>) -> Result<bool> {
        let affected = sqlx::query("UPDATE playlists SET folder_id = ? WHERE id = ?")
            .bind(new_folder)
            .bind(id)
            .execute(self.pool)
            .await?
            .rows_affected();
        Ok(affected > 0)
    }

    /// 删除歌单，返回是否命中。
    pub async fn delete_playlist(&self, id: i64) -> Result<bool> {
        let affected = sqlx::query("DELETE FROM playlists WHERE id = ?")
            .bind(id)
            .execute(self.pool)
            .await?
            .rows_affected();
        Ok(affected > 0)
    }

    /// 列举某用户的全部歌单（扁平，供 OpenSubsonic getPlaylists）。
    pub async fn list_playlists(&self, owner_id: i64) -> Result<Vec<Playlist>> {
        let rows: Vec<PlaylistRow> = sqlx::query_as(&format!(
            "{PLAYLIST_SELECT} WHERE p.owner_id = ? GROUP BY p.id ORDER BY p.position, p.name"
        ))
        .bind(owner_id)
        .fetch_all(self.pool)
        .await?;
        Ok(rows.into_iter().map(Playlist::from).collect())
    }

    // ── 歌单曲目 ────────────────────────────────────────────
    /// 用有序曲目整体替换歌单内容（事务内先清空再按序插入）。
    pub async fn set_tracks(&self, playlist_id: i64, track_ids: &[i64]) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("UPDATE playlists SET changed_at = datetime('now') WHERE id = ?")
            .bind(playlist_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM playlist_tracks WHERE playlist_id = ?")
            .bind(playlist_id)
            .execute(&mut *tx)
            .await?;
        for (pos, track_id) in track_ids.iter().enumerate() {
            sqlx::query(
                "INSERT INTO playlist_tracks(playlist_id, track_id, position) VALUES(?, ?, ?)",
            )
            .bind(playlist_id)
            .bind(track_id)
            .bind(pos as i64)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// 取歌单内有序曲目主键。
    pub async fn track_ids(&self, playlist_id: i64) -> Result<Vec<i64>> {
        sqlx::query_scalar(
            "SELECT track_id FROM playlist_tracks WHERE playlist_id = ? ORDER BY position",
        )
        .bind(playlist_id)
        .fetch_all(self.pool)
        .await
    }
}
