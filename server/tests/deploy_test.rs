//! 部署产物配置校验：Dockerfile / docker-compose.yml / .env.example（计划 T10）。
//!
//! 纯文件内容断言，不依赖 docker，保证 CI 可跑。校验一键部署的关键不变量：
//! server+garage 两服务、FFmpeg 打包进镜像、musl 静态构建、首启密钥与建管理员就位。

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/..");

fn read(rel: &str) -> String {
    std::fs::read_to_string(format!("{ROOT}/{rel}"))
        .unwrap_or_else(|e| panic!("读 {rel} 失败：{e}"))
}

#[test]
fn compose_含_server_与_garage_两服务且依赖正确() {
    let compose = read("docker-compose.yml");
    assert!(compose.contains("garage:"), "缺 garage 服务");
    assert!(compose.contains("server:"), "缺 server 服务");
    assert!(
        compose.contains("depends_on:"),
        "server 应 depends_on garage"
    );
    assert!(compose.contains("\"4533:4533\""), "应暴露 4533 端口");
    // FFmpeg 打包进 server 镜像，不作为独立服务。
    assert!(
        !compose.contains("ffmpeg:"),
        "FFmpeg 不应是独立 compose 服务"
    );
}

#[test]
fn compose_注入必需环境变量() {
    let compose = read("docker-compose.yml");
    for key in [
        "MUSIC_APP_SECRET",
        "MUSIC__GARAGE__ENDPOINT",
        "MUSIC__DATABASE__PATH",
        "MUSIC__SETUP__ADMIN_USERNAME",
    ] {
        assert!(compose.contains(key), "compose 缺环境变量 {key}");
    }
}

#[test]
fn dockerfile_为_musl_静态构建且附带_ffmpeg() {
    let dockerfile = read("Dockerfile");
    assert!(dockerfile.contains("alpine"), "应基于 musl(alpine)");
    assert!(
        dockerfile.contains("AS builder"),
        "应多阶段构建（builder + 运行）"
    );
    assert!(
        dockerfile.contains("crt-static"),
        "应静态链接（crt-static）"
    );
    assert!(
        dockerfile.contains("--bin music-server"),
        "应构建 music-server 二进制"
    );
    assert!(
        dockerfile.contains("ffmpeg"),
        "运行镜像应附带 FFmpeg（转码子进程）"
    );
}

#[test]
fn env_example_列出必填密钥() {
    let env = read(".env.example");
    for key in ["MUSIC_APP_SECRET", "GARAGE_ACCESS_KEY", "GARAGE_SECRET_KEY"] {
        assert!(env.contains(key), ".env.example 缺 {key}");
    }
}
