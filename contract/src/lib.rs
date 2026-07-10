//! 跨端共享数据类型（DTO）。
//!
//! 本 crate 只含**纯数据类型**，无业务逻辑，供服务端与所有客户端复用，
//! 保证接口类型不漂移。字段命名对齐 OpenSubsonic（序列化为 camelCase），
//! 便于 API 层直接产出 subsonic-response 信封。
//!
//! 时间戳统一用 ISO8601 字符串、标识符统一用不透明 `String`，
//! 避免引入 `chrono` 等依赖并利于后续 UniFFI 跨语言绑定。
//!
//! 安全：**绝不**在任何 DTO 暴露密码或对象存储内部键（`object_key`/`etag` 等）。

pub mod access;
pub mod media;
pub mod playlist;
pub mod response;
pub mod stream;
pub mod user;

pub use access::{AccessRule, Principal, PrincipalType, ScopeType};
pub use media::{Album, Artist, Genre, Track};
pub use playlist::{Playlist, PlaylistFolder};
pub use response::{ResponseStatus, SubsonicBody, SubsonicError, SubsonicResponse};
pub use stream::StreamRequest;
pub use user::{Role, User};
