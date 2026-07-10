//! HTTP API 层。
//!
//! T0 仅提供治理骨架路由；OpenSubsonic 兼容子集与自研扩展由后续任务填充。

mod system;

use axum::Router;

/// 构建 API 路由树。
pub fn router() -> Router {
    Router::new().merge(system::router())
}
