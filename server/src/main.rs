//! 音乐服务端二进制入口。

use std::net::SocketAddr;
use std::sync::Arc;

use yevune_server::api::AppState;
use yevune_server::config::Config;
use yevune_server::index::Index;
use yevune_server::setup::{ensure_admin, AdminSeed, SetupOutcome};
use yevune_server::storage::{GarageConfig, GarageStore, ObjectStore};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::load(Config::default_path().as_deref())?;

    yevune_server::init_tracing(&config.log.level);

    let index = Index::connect(&config.database.path).await?;
    let garage = GarageStore::new(GarageConfig {
        endpoint: config.garage.endpoint.clone(),
        bucket: config.garage.bucket.clone(),
        access_key: config.garage.access_key.clone(),
        secret_key: config.garage.secret_key.clone(),
        region: config.garage.region.clone(),
        allow_http: true,
        page_size: yevune_server::storage::DEFAULT_PAGE_SIZE,
    })?;
    let store: Arc<dyn ObjectStore> = Arc::new(garage);
    let app_secret = std::env::var("YEVUNE_APP_SECRET").map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "必须设置 YEVUNE_APP_SECRET（用于密码加密与会话签名）",
        )
    })?;
    if app_secret.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "YEVUNE_APP_SECRET 不得为空",
        )
        .into());
    }

    // 首启引导：无用户时创建管理员（幂等）。
    let seed = AdminSeed {
        username: config.setup.admin_username.clone(),
        password: config.setup.admin_password.clone(),
    };
    match ensure_admin(&index, &app_secret, &seed).await? {
        SetupOutcome::AlreadyInitialized => {}
        SetupOutcome::AdminCreated {
            username,
            generated_password: Some(password),
        } => tracing::warn!(
            %username,
            %password,
            "首启已创建管理员；请立即登录并修改此随机密码（仅此一次显示）"
        ),
        SetupOutcome::AdminCreated { username, .. } => {
            tracing::info!(%username, "首启已按配置创建管理员")
        }
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

    axum::serve(listener, yevune_server::app(state))
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

/// 等待 Ctrl-C 触发优雅关停。
async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("收到关停信号，正在退出");
}
