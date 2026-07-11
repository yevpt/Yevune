//! 面向原生 UI 的类型化客户端错误。

/// Core 操作错误。
#[derive(Debug, uniffi::Error)]
pub enum CoreError {
    /// 服务器地址不可用。
    InvalidServer { message: String },
    /// 网络请求或 JSON 解析失败。
    Network { message: String },
    /// 本地文件无法读取。
    File { message: String },
    /// 调用参数不满足端点约束。
    InvalidRequest { message: String },
    /// 尚未成功登录。
    NotAuthenticated,
    /// 服务端按 OpenSubsonic 信封报告的错误。
    Server { code: u32, message: String },
    /// 成功信封缺少端点要求的业务数据。
    InvalidResponse { message: String },
}

impl std::fmt::Display for CoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidServer { message }
            | Self::Network { message }
            | Self::File { message }
            | Self::InvalidRequest { message }
            | Self::InvalidResponse { message } => formatter.write_str(message),
            Self::NotAuthenticated => formatter.write_str("尚未登录"),
            Self::Server { code, message } => write!(formatter, "服务端错误 {code}: {message}"),
        }
    }
}

impl std::error::Error for CoreError {}

/// Core 通用结果别名。
pub type Result<T> = std::result::Result<T, CoreError>;
