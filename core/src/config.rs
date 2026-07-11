//! 服务器连接配置。

use crate::error::{CoreError, Result};

/// 用户提交的服务器配置。
#[derive(Debug, Clone)]
pub(crate) struct ServerConfig {
    base_url: reqwest::Url,
}

impl ServerConfig {
    /// 解析并规范化服务器根地址。
    pub(crate) fn parse(server: &str) -> Result<Self> {
        let mut base_url =
            reqwest::Url::parse(server).map_err(|error| CoreError::InvalidServer {
                message: error.to_string(),
            })?;
        if !matches!(base_url.scheme(), "http" | "https") || base_url.host().is_none() {
            return Err(CoreError::InvalidServer {
                message: "服务器地址必须是包含主机名的 HTTP(S) URL".to_owned(),
            });
        }
        base_url.set_query(None);
        base_url.set_fragment(None);
        if !base_url.path().ends_with('/') {
            let path = format!("{}/", base_url.path());
            base_url.set_path(&path);
        }
        Ok(Self { base_url })
    }

    pub(crate) fn endpoint(&self, name: &str) -> Result<reqwest::Url> {
        self.base_url
            .join(&format!("rest/{name}"))
            .map_err(|error| CoreError::InvalidServer {
                message: error.to_string(),
            })
    }

    pub(crate) fn public_url(&self) -> String {
        self.base_url.to_string().trim_end_matches('/').to_owned()
    }
}
