//! 服务端配置：TOML 文件 + 环境变量分层覆盖，全部字段带合理默认。
//!
//! 加载顺序（后者覆盖前者）：内建默认 → TOML 文件 → 环境变量（前缀 `MUSIC`，
//! 分隔符 `__`，如 `MUSIC__SERVER__PORT=8080`）。字段对齐设计文档 §11。

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// 顶层配置。
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    /// 监听地址与端口。
    pub server: ServerConfig,
    /// Garage(S3) 对象存储连接信息。
    pub garage: GarageConfig,
    /// SQLite 索引数据库（本地磁盘）。
    pub database: DatabaseConfig,
    /// 转码相关默认与缓存上限。
    pub transcode: TranscodeConfig,
    /// 扫描间隔。
    pub scan: ScanConfig,
    /// 首启建管理员引导。
    pub setup: SetupConfig,
    /// 可选 TLS 证书（默认明文 HTTP）。
    pub tls: Option<TlsConfig>,
    /// 日志级别。
    pub log: LogConfig,
}

/// 首启引导：无用户时创建管理员（设计文档 §11）。
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default)]
pub struct SetupConfig {
    /// 首启管理员用户名。
    pub admin_username: String,
    /// 首启管理员密码；留空则服务端生成随机密码并在启动日志中打印一次。
    pub admin_password: Option<String>,
}

impl Default for SetupConfig {
    fn default() -> Self {
        Self {
            admin_username: "admin".to_string(),
            admin_password: None,
        }
    }
}

/// 监听地址与端口。
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default)]
pub struct ServerConfig {
    /// 绑定主机地址。
    pub host: String,
    /// 绑定端口。
    pub port: u16,
}

/// Garage(S3) 连接信息。
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default)]
pub struct GarageConfig {
    /// S3 端点，如 `http://localhost:3900`。
    pub endpoint: String,
    /// 存放音频与转码缓存的 bucket。
    pub bucket: String,
    /// S3 region（Garage 通常为 `garage`）。
    pub region: String,
    /// 访问密钥 ID。
    pub access_key: String,
    /// 访问密钥 Secret。
    pub secret_key: String,
}

/// SQLite 索引数据库配置。
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default)]
pub struct DatabaseConfig {
    /// SQLite 文件路径（本地磁盘，红线：严禁放对象存储）。
    pub path: PathBuf,
}

/// 转码默认与缓存上限。
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default)]
pub struct TranscodeConfig {
    /// 转码缓存最大字节数（超出可 LRU 淘汰）。
    pub cache_max_bytes: u64,
    /// 默认转码格式。
    pub default_format: String,
    /// 默认转码码率（kbps）。
    pub default_bitrate: u32,
    /// FFmpeg 可执行文件路径。
    pub ffmpeg_path: PathBuf,
}

/// 扫描配置。
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default)]
pub struct ScanConfig {
    /// 定时扫描间隔（秒）。
    pub interval_seconds: u64,
}

/// 可选 TLS 证书配置。
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct TlsConfig {
    /// 证书文件路径（PEM）。
    pub cert_path: PathBuf,
    /// 私钥文件路径（PEM）。
    pub key_path: PathBuf,
}

/// 日志配置。
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default)]
pub struct LogConfig {
    /// 日志级别（`error`/`warn`/`info`/`debug`/`trace`）。
    pub level: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 4533,
        }
    }
}

impl Default for GarageConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:3900".to_string(),
            bucket: "music".to_string(),
            region: "garage".to_string(),
            access_key: String::new(),
            secret_key: String::new(),
        }
    }
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            path: PathBuf::from("./data/music.sqlite"),
        }
    }
}

impl Default for TranscodeConfig {
    fn default() -> Self {
        Self {
            // 10 GiB
            cache_max_bytes: 10 * 1024 * 1024 * 1024,
            default_format: "opus".to_string(),
            default_bitrate: 128,
            ffmpeg_path: PathBuf::from("ffmpeg"),
        }
    }
}

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            interval_seconds: 3600,
        }
    }
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
        }
    }
}

impl ServerConfig {
    /// 解析为可绑定的 [`SocketAddr`]。
    pub fn socket_addr(&self) -> Result<SocketAddr, std::net::AddrParseError> {
        format!("{}:{}", self.host, self.port).parse()
    }
}

impl Config {
    /// 默认配置文件路径：环境变量 `MUSIC_CONFIG` 优先，否则 `./config.toml`（若存在）。
    pub fn default_path() -> Option<PathBuf> {
        if let Ok(p) = std::env::var("MUSIC_CONFIG") {
            return Some(PathBuf::from(p));
        }
        let default = PathBuf::from("config.toml");
        default.exists().then_some(default)
    }

    /// 从可选 TOML 文件 + 环境变量加载配置。
    ///
    /// 文件不存在时静默跳过（仅用默认 + 环境变量）。
    pub fn load(path: Option<&Path>) -> Result<Config, config::ConfigError> {
        let env = config::Environment::with_prefix("MUSIC")
            .separator("__")
            .try_parsing(true);
        build(path, env)
    }
}

/// 实际的分层构建逻辑，`env` 可注入以便测试。
fn build(path: Option<&Path>, env: config::Environment) -> Result<Config, config::ConfigError> {
    let mut builder = config::Config::builder();
    if let Some(path) = path {
        builder = builder.add_source(config::File::from(path).required(false));
    }
    builder.add_source(env).build()?.try_deserialize()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env_from(pairs: &[(&str, &str)]) -> config::Environment {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        config::Environment::with_prefix("MUSIC")
            .separator("__")
            .try_parsing(true)
            .source(Some(map))
    }

    #[test]
    fn 默认值合理() {
        let cfg = build(None, env_from(&[])).unwrap();
        assert_eq!(cfg.server.host, "0.0.0.0");
        assert_eq!(cfg.server.port, 4533);
        assert_eq!(cfg.garage.bucket, "music");
        assert_eq!(cfg.database.path, PathBuf::from("./data/music.sqlite"));
        assert_eq!(cfg.scan.interval_seconds, 3600);
        assert_eq!(cfg.log.level, "info");
        assert!(cfg.tls.is_none());
    }

    #[test]
    fn toml_文件覆盖默认值() {
        let dir = std::env::temp_dir().join(format!("music-cfg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            r#"
[server]
port = 8080

[garage]
bucket = "familytunes"

[log]
level = "debug"
"#,
        )
        .unwrap();

        let cfg = build(Some(&path), env_from(&[])).unwrap();
        assert_eq!(cfg.server.port, 8080);
        assert_eq!(cfg.garage.bucket, "familytunes");
        assert_eq!(cfg.log.level, "debug");
        // 未指定字段仍取默认
        assert_eq!(cfg.server.host, "0.0.0.0");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn 环境变量覆盖文件与默认值() {
        let dir = std::env::temp_dir().join(format!("music-cfg-env-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, "[server]\nport = 8080\n").unwrap();

        let cfg = build(
            Some(&path),
            env_from(&[
                ("MUSIC__SERVER__PORT", "9999"),
                ("MUSIC__LOG__LEVEL", "trace"),
            ]),
        )
        .unwrap();
        // env 覆盖 TOML
        assert_eq!(cfg.server.port, 9999);
        // env 覆盖默认
        assert_eq!(cfg.log.level, "trace");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn socket_addr_可解析() {
        let cfg = Config::default();
        assert_eq!(cfg.server.socket_addr().unwrap().port(), 4533);
    }
}
