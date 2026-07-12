//! 跨平台音乐客户端核心。

mod api;
mod auth;
mod client;
mod config;
mod error;
mod ffi_types;
mod http;

pub use api::browse::{AlbumDetail, AlbumSort, ArtistDetail, SearchResult};
pub use api::manage::{TagUpdate, UploadMetadata, UploadProgress};
pub use api::scan::{DetailedScanResult, ScanAction, ScanChange, ScanStatus};
pub use client::{MusicClient, Session};
pub use error::CoreError;

uniffi::setup_scaffolding!();
