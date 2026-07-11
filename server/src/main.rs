//! 音乐服务端二进制入口。

use std::net::SocketAddr;

use music_server::config::Config;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::load(Config::default_path().as_deref())?;

    music_server::init_tracing(&config.log.level);

    let addr: SocketAddr = config.server.socket_addr()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "音乐服务端已启动");

    // TODO(T7 集成)：改挂 `music_server::app_with_state(state)` 以对外提供浏览/搜索/媒体
    // 端点。装配 AppState 需应用密钥（config 增字段）与 ObjectStore 初始化（GarageStore），
    // 属 T7/T10 集成范围；曲库访问控制强制已在 app_with_state 路径内实现并测试。
    axum::serve(listener, music_server::app())
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

/// 等待 Ctrl-C 触发优雅关停。
async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("收到关停信号，正在退出");
}
