//! 音乐服务端二进制入口。

use std::net::SocketAddr;
use std::sync::Arc;

use music_server::api::AppState;
use music_server::config::Config;
use music_server::index::Index;
use music_server::storage::{GarageConfig, GarageStore, ObjectStore};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::load(Config::default_path().as_deref())?;

    music_server::init_tracing(&config.log.level);

    let index = Index::connect(&config.database.path).await?;
    let garage = GarageStore::new(GarageConfig {
        endpoint: config.garage.endpoint.clone(),
        bucket: config.garage.bucket.clone(),
        access_key: config.garage.access_key.clone(),
        secret_key: config.garage.secret_key.clone(),
        region: config.garage.region.clone(),
        allow_http: true,
        page_size: music_server::storage::DEFAULT_PAGE_SIZE,
    })?;
    let store: Arc<dyn ObjectStore> = Arc::new(garage);
    let app_secret = std::env::var("MUSIC_APP_SECRET").map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "必须设置 MUSIC_APP_SECRET（用于密码加密与会话签名）",
        )
    })?;
    if app_secret.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "MUSIC_APP_SECRET 不得为空",
        )
        .into());
    }
    let state = AppState::with_transcode_defaults(
        index,
        store,
        &app_secret,
        config.transcode.ffmpeg_path.clone(),
        config.transcode.default_format.clone(),
        config.transcode.default_bitrate,
    );

    let addr: SocketAddr = config.server.socket_addr()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "音乐服务端已启动");

    axum::serve(listener, music_server::app(state))
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

/// 等待 Ctrl-C 触发优雅关停。
async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("收到关停信号，正在退出");
}
