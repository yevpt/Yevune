//! 带 OpenSubsonic 认证的 JSON HTTP 请求。

use serde::Deserialize;

use crate::auth::AuthenticatedSession;
use crate::error::{CoreError, Result};

/// Core 内部 HTTP 客户端。
#[derive(Debug, Clone)]
pub(crate) struct HttpClient {
    client: reqwest::Client,
}

#[derive(Debug, Deserialize)]
struct Envelope {
    #[serde(rename = "subsonic-response")]
    response: ResponseBody,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResponseBody {
    status: String,
    error: Option<ServerError>,
}

#[derive(Debug, Deserialize)]
struct ServerError {
    code: u32,
    message: String,
}

impl HttpClient {
    pub(crate) fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    /// 调用没有业务负载的 OpenSubsonic 端点并验证协议信封。
    pub(crate) async fn get_empty(
        &self,
        auth: &AuthenticatedSession,
        endpoint: &str,
    ) -> Result<()> {
        let mut url = auth.config.endpoint(endpoint)?;
        url.query_pairs_mut().extend_pairs(auth.query_pairs());
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(network_error)?
            .error_for_status()
            .map_err(network_error)?;
        let envelope: Envelope = response.json().await.map_err(network_error)?;
        if envelope.response.status == "ok" {
            return Ok(());
        }
        let error = envelope.response.error.unwrap_or(ServerError {
            code: 0,
            message: "服务端返回未知失败".to_owned(),
        });
        Err(CoreError::Server {
            code: error.code,
            message: error.message,
        })
    }
}

fn network_error(error: reqwest::Error) -> CoreError {
    CoreError::Network {
        message: error.to_string(),
    }
}
