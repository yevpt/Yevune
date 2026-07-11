//! 转码缓存登记仓储：对象本体在 Garage，本表只保存定位与 LRU 元数据。

use sqlx::{FromRow, SqlitePool};

use super::Result;

/// 已登记的转码缓存条目。
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct TranscodeCache {
    /// 原曲目主键。
    pub track_id: i64,
    /// 目标格式。
    pub format: String,
    /// 目标码率（kbps）。
    pub bitrate: u32,
    /// Garage 对象键。
    pub object_key: String,
    /// 缓存对象大小（字节）。
    pub size: u64,
    /// 创建时间。
    pub created_at: String,
    /// 最近访问时间。
    pub last_access: String,
}

/// 新建或更新缓存登记所需字段。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTranscodeCache {
    /// 原曲目主键。
    pub track_id: i64,
    /// 目标格式。
    pub format: String,
    /// 目标码率（kbps）。
    pub bitrate: u32,
    /// Garage 对象键。
    pub object_key: String,
    /// 缓存对象大小（字节）。
    pub size: u64,
}

/// `transcode_cache` 表的类型化访问入口。
pub struct TranscodeCacheRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> TranscodeCacheRepo<'a> {
    /// 绑定连接池。
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// 查询缓存并刷新 `last_access`；不存在返回 `None`。
    pub async fn get(
        &self,
        track_id: i64,
        format: &str,
        bitrate: u32,
    ) -> Result<Option<TranscodeCache>> {
        sqlx::query_as(
            "UPDATE transcode_cache SET last_access = datetime('now') \
             WHERE track_id = ? AND format = ? AND bitrate = ? \
             RETURNING track_id, format, bitrate, object_key, size, created_at, last_access",
        )
        .bind(track_id)
        .bind(format)
        .bind(bitrate as i64)
        .fetch_optional(self.pool)
        .await
    }

    /// 原子 upsert 一条完整缓存登记。
    pub async fn upsert(&self, entry: &NewTranscodeCache) -> Result<()> {
        sqlx::query(
            "INSERT INTO transcode_cache(track_id, format, bitrate, object_key, size) \
             VALUES(?, ?, ?, ?, ?) \
             ON CONFLICT(track_id, format, bitrate) DO UPDATE SET \
                 object_key = excluded.object_key, size = excluded.size, \
                 created_at = datetime('now'), last_access = datetime('now')",
        )
        .bind(entry.track_id)
        .bind(&entry.format)
        .bind(entry.bitrate as i64)
        .bind(&entry.object_key)
        .bind(entry.size as i64)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// 删除一条缓存登记；不存在视为成功。
    pub async fn remove(&self, track_id: i64, format: &str, bitrate: u32) -> Result<()> {
        sqlx::query(
            "DELETE FROM transcode_cache WHERE track_id = ? AND format = ? AND bitrate = ?",
        )
        .bind(track_id)
        .bind(format)
        .bind(bitrate as i64)
        .execute(self.pool)
        .await?;
        Ok(())
    }
}
