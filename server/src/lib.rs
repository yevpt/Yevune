//! 音乐服务端库入口。
//!
//! 对外暴露 [`config`] 配置加载、[`app`] 构建 axum 路由、[`init_tracing`] 初始化日志。
//! HTTP 路由与业务逻辑随后续任务逐步填充；本 crate 仅承载 T0 的治理骨架。

pub mod api;
pub mod auth;
pub mod config;
pub mod index;
pub mod scanner;
pub mod storage;

use axum::Router;

pub use api::AppState;

/// 构建无状态治理骨架应用（健康检查与 OpenSubsonic `ping`）。
pub fn app() -> Router {
    api::router()
}

/// 构建挂载业务端点的完整应用（浏览/搜索/媒体，均强制曲库访问控制）。
pub fn app_with_state(state: AppState) -> Router {
    api::router_with_state(state)
}

/// 初始化 `tracing` 结构化日志。
///
/// 日志级别优先读取 `RUST_LOG` 环境变量，否则回退到传入的 `default_level`。
/// 重复调用是安全的（首次生效，后续忽略）。
pub fn init_tracing(default_level: &str) {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};

    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_level));

    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer())
        .try_init();
}
