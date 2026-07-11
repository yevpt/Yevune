//! 流式播放请求参数（对齐 OpenSubsonic `stream`）。

use serde::{Deserialize, Serialize};

/// 播放/转码请求参数。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct StreamRequest {
    /// 目标曲目标识符。
    pub id: String,
    /// 期望格式，如 `aac`/`opus`/`raw`；`None` 表示由服务端按默认策略决定。
    pub format: Option<String>,
    /// 最大码率（kbps），对应 OpenSubsonic `maxBitRate`；`None` 表示不限。
    #[serde(rename = "maxBitRate")]
    pub max_bitrate: Option<u32>,
}
