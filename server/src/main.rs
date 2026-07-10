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
