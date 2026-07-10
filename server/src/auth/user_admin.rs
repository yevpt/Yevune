//! 用户/角色管理逻辑（供 T7/T8 的管理接口调用）。
//!
//! 封装密码加密这一横切关注点，并对内建角色等施加约束。授权（谁能调这些方法）由调用方
//! 用 [`super::AdminUser`] 提取器在服务端强制。
//!
//! 少数操作（重命名、内建角色判定）在 [`crate::index`] 的仓储未暴露对应方法，故经
//! [`Index::pool`](crate::index::Index::pool) 直接执行，仍限定在本模块内，不改动 index 层文件。

use super::password::Encryptor;
use super::AuthError;
use crate::index::Index;
use contract::{Role, User};

/// 内建管理员角色名。
pub const ROLE_ADMIN: &str = "admin";
/// 内建普通成员角色名。
pub const ROLE_MEMBER: &str = "member";

/// 用户/角色管理器。
pub struct UserAdmin<'a> {
    index: &'a Index,
    encryptor: &'a Encryptor,
}

impl<'a> UserAdmin<'a> {
    /// 绑定索引与加密器。
    pub fn new(index: &'a Index, encryptor: &'a Encryptor) -> Self {
        Self { index, encryptor }
    }

    /// 创建用户（密码加密存储）。`admin` 为真时赋予内建 `admin` 角色，否则 `member`。
    ///
    /// 相应内建角色不存在时自动创建（支持首启创建管理员，spec §1）。返回新用户 DTO。
    pub async fn create_user(
        &self,
        name: &str,
        password: &str,
        admin: bool,
    ) -> Result<User, AuthError> {
        let enc_pw = self.encryptor.encrypt(password);
        let id = self.index.users().create_user(name, &enc_pw).await?;
        let role_name = if admin { ROLE_ADMIN } else { ROLE_MEMBER };
        let role_id = self.ensure_builtin_role(role_name).await?;
        self.index.roles().assign(id, role_id).await?;
        self.index
            .users()
            .get_user(id)
            .await?
            .ok_or(AuthError::UnknownUser)
    }

    /// 重命名用户，返回是否命中。
    pub async fn update_user(&self, id: i64, new_name: &str) -> Result<bool, AuthError> {
        let affected = sqlx::query("UPDATE users SET name = ? WHERE id = ?")
            .bind(new_name)
            .bind(id)
            .execute(self.index.pool())
            .await?
            .rows_affected();
        Ok(affected > 0)
    }

    /// 删除用户，返回是否命中。
    pub async fn delete_user(&self, id: i64) -> Result<bool, AuthError> {
        Ok(self.index.users().delete_user(id).await?)
    }

    /// 修改密码（加密存储），返回是否命中用户。
    pub async fn change_password(&self, id: i64, new_password: &str) -> Result<bool, AuthError> {
        let enc_pw = self.encryptor.encrypt(new_password);
        Ok(self.index.users().change_password(id, &enc_pw).await?)
    }

    /// 创建自定义角色（非内建），返回角色 DTO。
    pub async fn create_role(&self, name: &str) -> Result<Role, AuthError> {
        let id = self.index.roles().create_role(name, false).await?;
        Ok(Role {
            id: id.to_string(),
            name: name.to_string(),
            is_builtin: false,
        })
    }

    /// 删除角色；内建角色不可删（[`AuthError::Forbidden`]）。返回是否命中。
    pub async fn delete_role(&self, id: i64) -> Result<bool, AuthError> {
        let is_builtin: Option<i64> =
            sqlx::query_scalar("SELECT is_builtin FROM roles WHERE id = ?")
                .bind(id)
                .fetch_optional(self.index.pool())
                .await?;
        match is_builtin {
            None => Ok(false),
            Some(b) if b != 0 => Err(AuthError::Forbidden),
            Some(_) => Ok(self.index.roles().delete_role(id).await?),
        }
    }

    /// 给用户分配角色（幂等）。
    pub async fn assign_role(&self, user_id: i64, role_id: i64) -> Result<(), AuthError> {
        Ok(self.index.roles().assign(user_id, role_id).await?)
    }

    /// 解除用户的某角色，返回是否命中。
    pub async fn unassign_role(&self, user_id: i64, role_id: i64) -> Result<bool, AuthError> {
        Ok(self.index.roles().unassign(user_id, role_id).await?)
    }

    /// 用户是否为管理员。
    pub async fn is_admin(&self, user_id: i64) -> Result<bool, AuthError> {
        Ok(self.index.roles().is_admin(user_id).await?)
    }

    /// 取内建角色 id，不存在则以内建标记创建。
    async fn ensure_builtin_role(&self, name: &str) -> Result<i64, AuthError> {
        if let Some(role) = self.index.roles().get_role_by_name(name).await? {
            return role.id.parse::<i64>().map_err(|_| AuthError::UnknownUser);
        }
        Ok(self.index.roles().create_role(name, true).await?)
    }
}
