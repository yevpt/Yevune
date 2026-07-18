//! 标注仓储：收藏/播放计数/评分，按用户隔离（设计文档 §6 `annotations`）。

use std::collections::HashMap;

use sqlx::{FromRow, QueryBuilder, Sqlite, SqlitePool};

use super::Result;

/// 某用户对某条目的标注快照。
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct Annotation {
    /// 收藏时间（`None` 表示未收藏）。
    pub starred_at: Option<String>,
    /// 播放次数。
    pub play_count: i64,
    /// 最近播放时间。
    pub last_played: Option<String>,
    /// 评分 1–5。
    pub rating: Option<i64>,
}

#[derive(FromRow)]
struct AnnotationRow {
    item_id: i64,
    starred_at: Option<String>,
    play_count: i64,
    last_played: Option<String>,
    rating: Option<i64>,
}

/// 标注仓储。
pub struct AnnotationRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> AnnotationRepo<'a> {
    /// 绑定连接池。
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// 收藏某条目。
    pub async fn star(&self, user_id: i64, item_type: &str, item_id: i64) -> Result<()> {
        sqlx::query(
            "INSERT INTO annotations(user_id, item_type, item_id, starred_at) \
             VALUES(?, ?, ?, datetime('now')) \
             ON CONFLICT(user_id, item_type, item_id) DO UPDATE SET starred_at = datetime('now')",
        )
        .bind(user_id)
        .bind(item_type)
        .bind(item_id)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// 取消收藏。
    pub async fn unstar(&self, user_id: i64, item_type: &str, item_id: i64) -> Result<()> {
        sqlx::query(
            "INSERT INTO annotations(user_id, item_type, item_id, starred_at) \
             VALUES(?, ?, ?, NULL) \
             ON CONFLICT(user_id, item_type, item_id) DO UPDATE SET starred_at = NULL",
        )
        .bind(user_id)
        .bind(item_type)
        .bind(item_id)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// 设置评分（`None` 清除）。
    pub async fn set_rating(
        &self,
        user_id: i64,
        item_type: &str,
        item_id: i64,
        rating: Option<u8>,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO annotations(user_id, item_type, item_id, rating) VALUES(?, ?, ?, ?) \
             ON CONFLICT(user_id, item_type, item_id) DO UPDATE SET rating = excluded.rating",
        )
        .bind(user_id)
        .bind(item_type)
        .bind(item_id)
        .bind(rating.map(|r| r as i64))
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// 记一次播放（播放计数 +1，更新最近播放时间）。
    pub async fn scrobble(&self, user_id: i64, item_type: &str, item_id: i64) -> Result<()> {
        self.scrobble_at(user_id, item_type, item_id, None).await
    }

    /// 记一次播放，可保留客户端上报的毫秒 Unix 时间。
    pub async fn scrobble_at(
        &self,
        user_id: i64,
        item_type: &str,
        item_id: i64,
        played_at_ms: Option<i64>,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO annotations(user_id, item_type, item_id, play_count, last_played) \
             VALUES(?, ?, ?, 1, COALESCE(datetime(? / 1000, 'unixepoch'), datetime('now'))) \
             ON CONFLICT(user_id, item_type, item_id) DO UPDATE SET \
                 play_count = play_count + 1, last_played = excluded.last_played",
        )
        .bind(user_id)
        .bind(item_type)
        .bind(item_id)
        .bind(played_at_ms)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// 取某用户对某条目的标注（无则 `None`）。
    pub async fn get(
        &self,
        user_id: i64,
        item_type: &str,
        item_id: i64,
    ) -> Result<Option<Annotation>> {
        sqlx::query_as(
            "SELECT starred_at, play_count, last_played, rating FROM annotations \
             WHERE user_id = ? AND item_type = ? AND item_id = ?",
        )
        .bind(user_id)
        .bind(item_type)
        .bind(item_id)
        .fetch_optional(self.pool)
        .await
    }

    /// 批量读取某用户对同类条目的标注，结果只包含数据库中已有的条目。
    pub async fn get_many(
        &self,
        user_id: i64,
        item_type: &str,
        item_ids: &[i64],
    ) -> Result<HashMap<i64, Annotation>> {
        if item_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let mut query = QueryBuilder::<Sqlite>::new(
            "SELECT item_id, starred_at, play_count, last_played, rating FROM annotations \
             WHERE user_id = ",
        );
        query
            .push_bind(user_id)
            .push(" AND item_type = ")
            .push_bind(item_type)
            .push(" AND item_id IN (");
        let mut separated = query.separated(", ");
        for id in item_ids {
            separated.push_bind(id);
        }
        separated.push_unseparated(")");

        let rows = query
            .build_query_as::<AnnotationRow>()
            .fetch_all(self.pool)
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| {
                (
                    row.item_id,
                    Annotation {
                        starred_at: row.starred_at,
                        play_count: row.play_count,
                        last_played: row.last_played,
                        rating: row.rating,
                    },
                )
            })
            .collect())
    }
}
