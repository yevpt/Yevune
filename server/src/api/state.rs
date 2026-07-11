//! 应用共享状态：索引 + 认证 + 对象存储。
//!
//! 各 handler 通过 axum 提取器从 [`AppState`] 取所需依赖；[`crate::auth::AuthState`]
//! 经 [`FromRef`] 暴露给认证提取器（[`crate::auth::CurrentUser`] 等）。

use std::sync::Arc;

use axum::extract::FromRef;

use crate::auth::AuthState;
use crate::index::Index;
use crate::storage::ObjectStore;

/// 应用共享状态（廉价 `Clone`：内部均为句柄/`Arc`）。
#[derive(Clone)]
pub struct AppState {
    /// 元数据索引（SQLite 连接池句柄）。
    pub index: Index,
    /// 认证状态（密码加密器 + 令牌密钥 + 索引）。
    pub auth: AuthState,
    /// 对象存储（Garage/内存假实现）。
    pub store: Arc<dyn ObjectStore>,
}

impl AppState {
    /// 由索引、应用密钥与对象存储装配。认证状态从索引 + 密钥派生。
    pub fn new(index: Index, app_secret: &str, store: Arc<dyn ObjectStore>) -> Self {
        let auth = AuthState::new(index.clone(), app_secret);
        Self { index, auth, store }
    }
}

impl FromRef<AppState> for AuthState {
    fn from_ref(state: &AppState) -> Self {
        state.auth.clone()
    }
}
