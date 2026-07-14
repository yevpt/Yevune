//! 曲库访问控制仓储（设计文档 §6「曲库访问控制模型」）。
//!
//! 默认开放：仅为被限制内容存规则。规则 = 作用域 + 允许名单；
//! 查询时按最具体作用域优先评估（track > album > artist > genre）。

use contract::{AccessRule, Principal, PrincipalType, ScopeType};
use sqlx::SqlitePool;

use super::Result;

/// 作用域类型 → 存储字符串。
fn scope_str(s: ScopeType) -> &'static str {
    match s {
        ScopeType::Track => "track",
        ScopeType::Album => "album",
        ScopeType::Artist => "artist",
        ScopeType::Genre => "genre",
    }
}

/// 存储字符串 → 作用域类型。
fn parse_scope(s: &str) -> Option<ScopeType> {
    match s {
        "track" => Some(ScopeType::Track),
        "album" => Some(ScopeType::Album),
        "artist" => Some(ScopeType::Artist),
        "genre" => Some(ScopeType::Genre),
        _ => None,
    }
}

/// 主体类型 → 存储字符串。
fn principal_str(p: PrincipalType) -> &'static str {
    match p {
        PrincipalType::User => "user",
        PrincipalType::Role => "role",
    }
}

/// 存储字符串 → 主体类型。
fn parse_principal(s: &str) -> Option<PrincipalType> {
    match s {
        "user" => Some(PrincipalType::User),
        "role" => Some(PrincipalType::Role),
        _ => None,
    }
}

/// 定位一条曲目所属层级、用于评估可见性的键。
#[derive(Debug, Clone, Default)]
pub struct TrackScope<'a> {
    /// 曲目主键。
    pub track_id: i64,
    /// 所属专辑主键。
    pub album_id: Option<i64>,
    /// 艺人主键。
    pub artist_id: Option<i64>,
    /// 流派名。
    pub genre: Option<&'a str>,
}

/// 访问控制仓储。
pub struct AccessRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> AccessRepo<'a> {
    /// 绑定连接池。
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// upsert 一条规则并整体替换其允许名单，返回规则主键。
    pub async fn set_rule(
        &self,
        scope_type: ScopeType,
        scope_id: &str,
        created_by: Option<i64>,
        grants: &[Principal],
    ) -> Result<i64> {
        let mut tx = self.pool.begin().await?;
        let rule_id: i64 = sqlx::query_scalar(
            "INSERT INTO access_rules(scope_type, scope_id, created_by) VALUES(?, ?, ?) \
             ON CONFLICT(scope_type, scope_id) DO UPDATE SET created_by = excluded.created_by \
             RETURNING id",
        )
        .bind(scope_str(scope_type))
        .bind(scope_id)
        .bind(created_by)
        .fetch_one(&mut *tx)
        .await?;

        sqlx::query("DELETE FROM access_rule_grants WHERE rule_id = ?")
            .bind(rule_id)
            .execute(&mut *tx)
            .await?;
        for g in grants {
            let pid: i64 = g.id.parse().unwrap_or_default();
            sqlx::query(
                "INSERT INTO access_rule_grants(rule_id, principal_type, principal_id) \
                 VALUES(?, ?, ?)",
            )
            .bind(rule_id)
            .bind(principal_str(g.principal_type))
            .bind(pid)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(rule_id)
    }

    /// 取某作用域的规则（含允许名单）。
    pub async fn get_rule(
        &self,
        scope_type: ScopeType,
        scope_id: &str,
    ) -> Result<Option<AccessRule>> {
        let row: Option<(i64, String, String)> = sqlx::query_as(
            "SELECT id, scope_type, scope_id FROM access_rules \
             WHERE scope_type = ? AND scope_id = ?",
        )
        .bind(scope_str(scope_type))
        .bind(scope_id)
        .fetch_optional(self.pool)
        .await?;

        match row {
            Some(r) => Ok(Some(self.hydrate_rule(r).await?)),
            None => Ok(None),
        }
    }

    /// 删除某作用域的规则，返回是否命中。
    pub async fn delete_rule(&self, scope_type: ScopeType, scope_id: &str) -> Result<bool> {
        let affected =
            sqlx::query("DELETE FROM access_rules WHERE scope_type = ? AND scope_id = ?")
                .bind(scope_str(scope_type))
                .bind(scope_id)
                .execute(self.pool)
                .await?
                .rows_affected();
        Ok(affected > 0)
    }

    /// 列举全部规则。
    pub async fn list_rules(&self) -> Result<Vec<AccessRule>> {
        let rows: Vec<(i64, String, String)> =
            sqlx::query_as("SELECT id, scope_type, scope_id FROM access_rules ORDER BY id")
                .fetch_all(self.pool)
                .await?;
        let mut out = Vec::with_capacity(rows.len());
        for r in rows {
            out.push(self.hydrate_rule(r).await?);
        }
        Ok(out)
    }

    /// 评估某曲目适用的规则：最具体作用域优先，无则返回 `None`（开放）。
    pub async fn effective_rule(&self, scope: &TrackScope<'_>) -> Result<Option<AccessRule>> {
        // track > album > artist > genre
        if let Some(rule) = self
            .get_rule(ScopeType::Track, &scope.track_id.to_string())
            .await?
        {
            return Ok(Some(rule));
        }
        if let Some(album_id) = scope.album_id {
            if let Some(rule) = self
                .get_rule(ScopeType::Album, &album_id.to_string())
                .await?
            {
                return Ok(Some(rule));
            }
        }
        if let Some(artist_id) = scope.artist_id {
            if let Some(rule) = self
                .get_rule(ScopeType::Artist, &artist_id.to_string())
                .await?
            {
                return Ok(Some(rule));
            }
        }
        if let Some(genre) = scope.genre {
            if let Some(rule) = self.get_rule(ScopeType::Genre, genre).await? {
                return Ok(Some(rule));
            }
        }
        Ok(None)
    }

    /// 把规则行补全为 DTO（附允许名单）。
    async fn hydrate_rule(&self, row: (i64, String, String)) -> Result<AccessRule> {
        let (id, scope_type, scope_id) = row;
        let grant_rows: Vec<(String, i64)> = sqlx::query_as(
            "SELECT principal_type, principal_id FROM access_rule_grants WHERE rule_id = ?",
        )
        .bind(id)
        .fetch_all(self.pool)
        .await?;
        let grants = grant_rows
            .into_iter()
            .filter_map(|(pt, pid)| {
                parse_principal(&pt).map(|principal_type| Principal {
                    principal_type,
                    id: pid.to_string(),
                })
            })
            .collect();
        Ok(AccessRule {
            id: id.to_string(),
            scope_type: parse_scope(&scope_type).unwrap_or(ScopeType::Track),
            scope_id,
            scope_name: None,
            grants,
        })
    }
}
