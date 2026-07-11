//! OpenSubsonic 请求认证参数。

use crate::config::ServerConfig;

/// 已认证连接的私有凭证载体，不越过 FFI。
#[derive(Debug, Clone)]
pub(crate) struct AuthenticatedSession {
    pub(crate) config: ServerConfig,
    pub(crate) user: String,
    pub(crate) password: String,
}

impl AuthenticatedSession {
    pub(crate) fn query_pairs(&self) -> [(&str, &str); 5] {
        [
            ("u", &self.user),
            ("p", &self.password),
            ("v", "1.16.1"),
            ("c", "music-mac"),
            ("f", "json"),
        ]
    }
}
