//! 原生客户端扩展 API；所有路由严格隔离于 `/rest/ext/*`。

mod access;
mod cover;
mod library;
mod playlist_tree;
mod role;
mod scan;
mod user;

use axum::Router;

use super::AppState;

/// 构建自研扩展路由。
pub(super) fn router() -> Router<AppState> {
    Router::new()
        .merge(playlist_tree::router())
        .merge(cover::router())
        .merge(library::router())
        .merge(access::router())
        .merge(role::router())
        .merge(user::router())
        .merge(scan::router())
}
