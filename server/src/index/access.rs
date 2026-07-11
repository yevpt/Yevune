//! 曲库访问控制**强制**（设计文档 §6「曲库访问控制模型」）。
//!
//! 与 [`super::repo_access`]（规则的读写仓储）分工：本模块只做**查询时可见性判定**，
//! 产出可注入所有曲库读路径的 SQL 谓词与判定函数，授权在服务端强制、客户端不可绕过。
//!
//! 判定语义：
//! - **默认开放**：曲目在其 曲目/专辑/艺人/流派 层级都无规则 → 可见。
//! - **最具体优先**：track > album > artist > genre，取最具体的**存在**规则定胜负。
//! - **允许名单**：命中规则时，仅当访问者（用户或其角色）在该规则允许名单内才可见。
//! - **管理员绕过**：管理员永远可见全部。
//! - **查询时评估**：不逐曲固化，新入库曲目自动继承其专辑/艺人/流派规则。

use sqlx::SqlitePool;

use super::Result;

/// 解析后的访问者主体集合。判定所需的一切都在此：用户 id、其角色 id 集、是否管理员。
///
/// 由 [`AccessControl::resolve_viewer`] 从索引解析（**服务端强制**，不信任客户端自述角色）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Viewer {
    /// 用户主键。
    pub user_id: i64,
    /// 用户所属角色主键集合。
    pub role_ids: Vec<i64>,
    /// 是否管理员（拥有内建 `admin` 角色）。
    pub admin: bool,
}

/// 访问控制判定器，绑定连接池。
pub struct AccessControl<'a> {
    pool: &'a SqlitePool,
}

impl<'a> AccessControl<'a> {
    /// 绑定连接池。
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// 从索引解析访问者（角色集 + 管理员标记）。授权信息一律以服务端为准。
    pub async fn resolve_viewer(&self, user_id: i64) -> Result<Viewer> {
        let role_ids: Vec<i64> =
            sqlx::query_scalar("SELECT role_id FROM user_roles WHERE user_id = ?")
                .bind(user_id)
                .fetch_all(self.pool)
                .await?;
        let admin: i64 = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM user_roles ur JOIN roles r ON r.id = ur.role_id \
             WHERE ur.user_id = ? AND r.name = 'admin')",
        )
        .bind(user_id)
        .fetch_one(self.pool)
        .await?;
        Ok(Viewer {
            user_id,
            role_ids,
            admin: admin != 0,
        })
    }

    /// 单曲目可见性判定：`true` 表示 `viewer` 可访问该曲目。
    pub async fn can_access_track(&self, viewer: &Viewer, track_id: i64) -> Result<bool> {
        if viewer.admin {
            return Ok(true);
        }
        let sql = format!(
            "SELECT EXISTS(SELECT 1 FROM tracks t WHERE t.id = ? AND ({}))",
            self.visibility_sql(viewer)
        );
        let visible: i64 = sqlx::query_scalar(&sql)
            .bind(track_id)
            .fetch_one(self.pool)
            .await?;
        Ok(visible != 0)
    }

    /// 生成作用于曲目别名 `t` 的可见性谓词。见 [`visibility_sql_for`](Self::visibility_sql_for)。
    pub fn visibility_sql(&self, viewer: &Viewer) -> String {
        self.visibility_sql_for(viewer, "t")
    }

    /// 生成可注入曲目读查询的**可见性谓词**（布尔 SQL 表达式，作用于曲目别名 `alias`）。
    ///
    /// 供 browsing/search/media 等所有读路径在 `WHERE` 中 `AND (谓词)` 使用，
    /// 从而把"查询时评估 + 最具体优先 + 管理员绕过"统一强制在数据层。
    /// 需要在 `EXISTS` 子查询中判定另一张曲目别名（如按专辑聚可见曲目）时传入对应别名。
    pub fn visibility_sql_for(&self, viewer: &Viewer, alias: &str) -> String {
        if viewer.admin {
            return "1 = 1".to_string();
        }
        let m = self.principal_match(viewer);
        // 各层级裁决：命中规则→1(允许)/0(拒绝)，无规则→NULL(向下一层回退)。
        // COALESCE 取最具体的非空裁决；全空则默认开放(1)。
        format!(
            "COALESCE({track}, {album}, {artist}, {genre}, 1) = 1",
            track = level_verdict("track", &format!("CAST({alias}.id AS TEXT)"), &m),
            album = level_verdict("album", &format!("CAST({alias}.album_id AS TEXT)"), &m),
            artist = level_verdict("artist", &format!("CAST({alias}.artist_id AS TEXT)"), &m),
            genre = level_verdict("genre", &format!("{alias}.genre"), &m),
        )
    }

    /// 访问者主体匹配子句：命中规则允许名单中的用户或其任一角色。
    fn principal_match(&self, viewer: &Viewer) -> String {
        let mut clauses = vec![format!(
            "(g.principal_type = 'user' AND g.principal_id = {})",
            viewer.user_id
        )];
        if !viewer.role_ids.is_empty() {
            let ids = viewer
                .role_ids
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(",");
            clauses.push(format!(
                "(g.principal_type = 'role' AND g.principal_id IN ({ids}))"
            ));
        }
        clauses.join(" OR ")
    }
}

/// 某一作用域层级对曲目的裁决子查询：命中规则返回 1/0，无规则返回 NULL。
///
/// `scope_expr` 为该层级作用域标识在曲目行上的取值表达式（如 `CAST(t.album_id AS TEXT)`）。
/// `access_rules` 对 `(scope_type, scope_id)` 唯一，故标量子查询至多一行、安全。
fn level_verdict(scope_type: &str, scope_expr: &str, principal_match: &str) -> String {
    format!(
        "(SELECT CASE WHEN EXISTS(\
             SELECT 1 FROM access_rule_grants g WHERE g.rule_id = r.id AND ({principal_match})\
         ) THEN 1 ELSE 0 END \
         FROM access_rules r \
         WHERE r.scope_type = '{scope_type}' AND r.scope_id = {scope_expr})"
    )
}
