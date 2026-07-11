//! 带 OpenSubsonic 认证的 JSON HTTP 请求。

use crate::api::manage::{self, UploadProgress};
use crate::auth::AuthenticatedSession;
use crate::error::{CoreError, Result};
use serde::de::DeserializeOwned;
use serde::Deserialize;

/// Core 内部 HTTP 客户端。
#[derive(Debug, Clone)]
pub(crate) struct HttpClient {
    client: reqwest::Client,
}

#[derive(Debug, Deserialize)]
struct Envelope<T> {
    #[serde(rename = "subsonic-response")]
    response: ResponseBody<T>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResponseBody<T> {
    status: String,
    error: Option<ServerError>,
    #[serde(flatten)]
    data: T,
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

    /// 从本地路径流式读取 multipart 文件并上传至曲库扩展端点。
    pub(crate) async fn upload_track(
        &self,
        auth: &AuthenticatedSession,
        local_path: String,
        library_key: String,
        progress: Box<dyn UploadProgress>,
    ) -> Result<contract::Track> {
        let mut url = auth.config.endpoint("ext/uploadTrack")?;
        {
            let mut query = url.query_pairs_mut();
            query.extend_pairs(auth.query_pairs());
        }
        tokio::task::spawn_blocking(move || {
            let client = reqwest::blocking::Client::new();
            manage::blocking_upload(&client, url, local_path, library_key, progress)
        })
        .await
        .map_err(|error| CoreError::Network {
            message: error.to_string(),
        })?
    }

    /// 调用没有业务负载的 OpenSubsonic 端点并验证协议信封。
    pub(crate) async fn get_empty(
        &self,
        auth: &AuthenticatedSession,
        endpoint: &str,
    ) -> Result<()> {
        self.get_json::<EmptyPayload>(auth, endpoint, &[]).await?;
        Ok(())
    }

    /// 发送认证 GET 请求并提取 OpenSubsonic 成功信封中的业务数据。
    pub(crate) async fn get_json<T>(
        &self,
        auth: &AuthenticatedSession,
        endpoint: &str,
        parameters: &[(String, String)],
    ) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let mut url = auth.config.endpoint(endpoint)?;
        {
            let mut query = url.query_pairs_mut();
            query.extend_pairs(auth.query_pairs());
            for (key, value) in parameters {
                query.append_pair(key, value);
            }
        }
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(network_error)?
            .error_for_status()
            .map_err(network_error)?;
        let envelope: Envelope<T> = response.json().await.map_err(network_error)?;
        if envelope.response.status == "ok" {
            return Ok(envelope.response.data);
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

#[derive(Deserialize)]
struct EmptyPayload {}

fn network_error(error: reqwest::Error) -> CoreError {
    CoreError::Network {
        message: error.to_string(),
    }
}
