//! HTTP API 层。
//!
//! [`router`] 为无状态治理骨架（健康检查 / `ping`）；[`router_with_state`] 挂载依赖
//! [`AppState`] 的业务端点（浏览/搜索/媒体，均已强制曲库访问控制）。

mod browsing;
mod media;
mod response;
mod search;
pub mod state;
mod system;

use axum::Router;

pub use state::AppState;

/// 构建无状态治理骨架路由（`/healthz`、`/rest/ping`）。
pub fn router() -> Router {
    Router::new().merge(system::router())
}

/// 构建挂载业务端点的完整路由树（需要 [`AppState`]）。
pub fn router_with_state(state: AppState) -> Router {
    // 先合并需要状态的子路由并注入状态，再与无状态骨架合并。
    let stateful = Router::new()
        .merge(browsing::router())
        .merge(search::router())
        .merge(media::router())
        .with_state(state);
    router().merge(stateful)
}
