//! 用户与角色仓储。
//!
//! 密码以 `password_enc` 存储、**绝不**进入 DTO；DTO 仅暴露 id/name/created/admin/roles。

use contract::{Role, User};
use sqlx::{FromRow, SqlitePool};

use super::Result;

#[derive(FromRow)]
struct UserRow {
    id: i64,
    name: String,
    created_at: String,
}

#[derive(FromRow)]
struct RoleRow {
    id: i64,
    name: String,
    is_builtin: i64,
}

impl From<RoleRow> for Role {
    fn from(r: RoleRow) -> Self {
        Role {
            id: r.id.to_string(),
            name: r.name,
            is_builtin: r.is_builtin != 0,
        }
    }
}

/// 用户仓储。
pub struct UserRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> UserRepo<'a> {
    /// 绑定连接池。
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// 创建用户，返回主键。
    pub async fn create_user(&self, name: &str, password_enc: &str) -> Result<i64> {
        sqlx::query_scalar("INSERT INTO users(name, password_enc) VALUES(?, ?) RETURNING id")
            .bind(name)
            .bind(password_enc)
            .fetch_one(self.pool)
            .await
    }

    /// 按主键取用户 DTO（含角色与 admin 标记）。
    pub async fn get_user(&self, id: i64) -> Result<Option<User>> {
        let row: Option<UserRow> =
            sqlx::query_as("SELECT id, name, created_at FROM users WHERE id = ?")
                .bind(id)
                .fetch_optional(self.pool)
                .await?;
        self.build_user(row).await
    }

    /// 按用户名取用户 DTO。
    pub async fn get_user_by_name(&self, name: &str) -> Result<Option<User>> {
        let row: Option<UserRow> =
            sqlx::query_as("SELECT id, name, created_at FROM users WHERE name = ?")
                .bind(name)
                .fetch_optional(self.pool)
                .await?;
        self.build_user(row).await
    }

    /// 列举全部用户。
    pub async fn list_users(&self) -> Result<Vec<User>> {
        let rows: Vec<UserRow> =
            sqlx::query_as("SELECT id, name, created_at FROM users ORDER BY name")
                .fetch_all(self.pool)
                .await?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            if let Some(u) = self.build_user(Some(row)).await? {
                out.push(u);
            }
        }
        Ok(out)
    }

    /// 改密码，返回是否命中用户。
    pub async fn change_password(&self, id: i64, password_enc: &str) -> Result<bool> {
        let affected = sqlx::query("UPDATE users SET password_enc = ? WHERE id = ?")
            .bind(password_enc)
            .bind(id)
            .execute(self.pool)
            .await?
            .rows_affected();
        Ok(affected > 0)
    }

    /// 删除用户，返回是否删除到行。
    pub async fn delete_user(&self, id: i64) -> Result<bool> {
        let affected = sqlx::query("DELETE FROM users WHERE id = ?")
            .bind(id)
            .execute(self.pool)
            .await?
            .rows_affected();
        Ok(affected > 0)
    }

    /// 取用户名对应的 `password_enc`（供认证层校验，不进入 DTO）。
    pub async fn password_enc(&self, name: &str) -> Result<Option<String>> {
        sqlx::query_scalar("SELECT password_enc FROM users WHERE name = ?")
            .bind(name)
            .fetch_optional(self.pool)
            .await
    }

    /// 把用户行补全为 DTO（附角色名与 admin 标记）。
    async fn build_user(&self, row: Option<UserRow>) -> Result<Option<User>> {
        let Some(row) = row else { return Ok(None) };
        let roles: Vec<String> = sqlx::query_scalar(
            "SELECT r.name FROM roles r \
             JOIN user_roles ur ON ur.role_id = r.id \
             WHERE ur.user_id = ? ORDER BY r.name",
        )
        .bind(row.id)
        .fetch_all(self.pool)
        .await?;
        let admin = roles.iter().any(|r| r == "admin");
        Ok(Some(User {
            id: row.id.to_string(),
            name: row.name,
            created: Some(row.created_at),
            admin,
            roles,
        }))
    }
}

/// 角色仓储。
pub struct RoleRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> RoleRepo<'a> {
    /// 绑定连接池。
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// 创建角色，返回主键。
    pub async fn create_role(&self, name: &str, is_builtin: bool) -> Result<i64> {
        sqlx::query_scalar("INSERT INTO roles(name, is_builtin) VALUES(?, ?) RETURNING id")
            .bind(name)
            .bind(is_builtin as i64)
            .fetch_one(self.pool)
            .await
    }

    /// 按名取角色。
    pub async fn get_role_by_name(&self, name: &str) -> Result<Option<Role>> {
        let row: Option<RoleRow> =
            sqlx::query_as("SELECT id, name, is_builtin FROM roles WHERE name = ?")
                .bind(name)
                .fetch_optional(self.pool)
                .await?;
        Ok(row.map(Role::from))
    }

    /// 列举全部角色。
    pub async fn list_roles(&self) -> Result<Vec<Role>> {
        let rows: Vec<RoleRow> =
            sqlx::query_as("SELECT id, name, is_builtin FROM roles ORDER BY name")
                .fetch_all(self.pool)
                .await?;
        Ok(rows.into_iter().map(Role::from).collect())
    }

    /// 删除角色，返回是否删除到行。
    pub async fn delete_role(&self, id: i64) -> Result<bool> {
        let affected = sqlx::query("DELETE FROM roles WHERE id = ?")
            .bind(id)
            .execute(self.pool)
            .await?
            .rows_affected();
        Ok(affected > 0)
    }

    /// 给用户分配角色（幂等）。
    pub async fn assign(&self, user_id: i64, role_id: i64) -> Result<()> {
        sqlx::query(
            "INSERT INTO user_roles(user_id, role_id) VALUES(?, ?) \
             ON CONFLICT(user_id, role_id) DO NOTHING",
        )
        .bind(user_id)
        .bind(role_id)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// 解除用户的某角色，返回是否命中。
    pub async fn unassign(&self, user_id: i64, role_id: i64) -> Result<bool> {
        let affected = sqlx::query("DELETE FROM user_roles WHERE user_id = ? AND role_id = ?")
            .bind(user_id)
            .bind(role_id)
            .execute(self.pool)
            .await?
            .rows_affected();
        Ok(affected > 0)
    }

    /// 取用户的全部角色。
    pub async fn roles_of(&self, user_id: i64) -> Result<Vec<Role>> {
        let rows: Vec<RoleRow> = sqlx::query_as(
            "SELECT r.id, r.name, r.is_builtin FROM roles r \
             JOIN user_roles ur ON ur.role_id = r.id \
             WHERE ur.user_id = ? ORDER BY r.name",
        )
        .bind(user_id)
        .fetch_all(self.pool)
        .await?;
        Ok(rows.into_iter().map(Role::from).collect())
    }

    /// 用户是否为管理员（拥有内建 `admin` 角色）。
    pub async fn is_admin(&self, user_id: i64) -> Result<bool> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM user_roles ur \
             JOIN roles r ON r.id = ur.role_id \
             WHERE ur.user_id = ? AND r.name = 'admin'",
        )
        .bind(user_id)
        .fetch_one(self.pool)
        .await?;
        Ok(count > 0)
    }
}
