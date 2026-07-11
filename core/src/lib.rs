//! 跨平台音乐客户端核心。

mod auth;
mod client;
mod config;
mod error;
mod http;

pub use client::{MusicClient, Session};
pub use error::CoreError;

uniffi::setup_scaffolding!();
