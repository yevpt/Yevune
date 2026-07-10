//! 曲库访问控制类型（设计文档 §6「曲库访问控制模型」）。
//!
//! 规则 = 作用域 + 允许名单；默认开放，仅为被限制内容存规则。

use serde::{Deserialize, Serialize};

/// 访问规则作用域类型，最具体优先（track > album > artist > genre）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScopeType {
    /// 单曲目。
    Track,
    /// 整张专辑。
    Album,
    /// 整个艺人。
    Artist,
    /// 整个流派。
    Genre,
}

/// 被授权主体的类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PrincipalType {
    /// 具体用户。
    User,
    /// 角色（其下所有用户）。
    Role,
}

/// 允许名单中的一个主体（用户或角色）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Principal {
    /// 主体类型（JSON 字段名为 `type`）。
    #[serde(rename = "type")]
    pub principal_type: PrincipalType,
    /// 主体标识符。
    pub id: String,
}

/// 一条访问规则：某作用域仅对允许名单内主体可见。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessRule {
    /// 不透明标识符。
    pub id: String,
    /// 作用域类型。
    pub scope_type: ScopeType,
    /// 作用域目标标识符（曲目/专辑/艺人 id，或流派名）。
    pub scope_id: String,
    /// 允许访问的主体名单。
    pub grants: Vec<Principal>,
}
