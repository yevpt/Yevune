//! 用户与角色（设计文档 §6 `users`/`roles`）。
//!
//! 安全：**绝不**在 DTO 暴露密码（`password_enc`）。

use serde::{Deserialize, Serialize};

/// 用户。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    /// 不透明标识符。
    pub id: String,
    /// 用户名（OpenSubsonic 请求中的 `u`）。
    pub name: String,
    /// 创建时间（ISO8601），对应 `created_at`。
    pub created: Option<String>,
    /// 是否管理员（便于客户端；服务端仍以角色为准强制授权）。
    pub admin: bool,
    /// 所属角色名列表。
    pub roles: Vec<String>,
}

/// 角色（内建 `admin`/`member` 或管理员自建）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Role {
    /// 不透明标识符。
    pub id: String,
    /// 角色名。
    pub name: String,
    /// 是否内建角色（内建不可删）。
    pub is_builtin: bool,
}
