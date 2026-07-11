//! OpenSubsonic 统一响应信封 `subsonic-response`（成功/错误）。

use serde::{Deserialize, Serialize};

/// 声明兼容的 OpenSubsonic/Subsonic 协议版本。
pub const OPEN_SUBSONIC_API_VERSION: &str = "1.16.1";

/// 响应状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ResponseStatus {
    /// 成功。
    Ok,
    /// 失败（附 `error`）。
    Failed,
}

/// 错误详情，对齐 OpenSubsonic error 对象。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SubsonicError {
    /// OpenSubsonic 错误码。
    pub code: u32,
    /// 人类可读错误信息。
    pub message: String,
}

/// `subsonic-response` 对象体。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubsonicBody<T> {
    /// 状态。
    pub status: ResponseStatus,
    /// 协议版本。
    pub version: String,
    /// 服务端类型标识（JSON 字段名为 `type`）。
    #[serde(rename = "type")]
    pub server_type: String,
    /// 服务端软件版本。
    pub server_version: String,
    /// 是否支持 OpenSubsonic 扩展。
    pub open_subsonic: bool,
    /// 错误详情（仅失败时）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<SubsonicError>,
    /// 成功载荷（扁平并入本对象）。
    #[serde(flatten)]
    pub data: Option<T>,
}

/// 顶层信封 `{ "subsonic-response": { ... } }`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubsonicResponse<T> {
    /// 响应体。
    #[serde(rename = "subsonic-response")]
    pub body: SubsonicBody<T>,
}

impl<T> SubsonicResponse<T> {
    /// 构造成功响应，`version` 取协议常量、`openSubsonic=true`。
    pub fn ok(server_type: impl Into<String>, server_version: impl Into<String>, data: T) -> Self {
        Self {
            body: SubsonicBody {
                status: ResponseStatus::Ok,
                version: OPEN_SUBSONIC_API_VERSION.to_string(),
                server_type: server_type.into(),
                server_version: server_version.into(),
                open_subsonic: true,
                error: None,
                data: Some(data),
            },
        }
    }

    /// 构造失败响应。
    pub fn failed(
        server_type: impl Into<String>,
        server_version: impl Into<String>,
        error: SubsonicError,
    ) -> Self {
        Self {
            body: SubsonicBody {
                status: ResponseStatus::Failed,
                version: OPEN_SUBSONIC_API_VERSION.to_string(),
                server_type: server_type.into(),
                server_version: server_version.into(),
                open_subsonic: true,
                error: Some(error),
                data: None,
            },
        }
    }
}
