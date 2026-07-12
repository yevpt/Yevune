//! 生成 `openapi.yaml`（仓库根）。CI/开发者据此为 web 端生成 TS 类型。
//!
//! 用法：`cargo run --bin gen_openapi`（写入 `../openapi.yaml`），或传路径参数指定输出位置。

use std::path::PathBuf;

fn main() -> std::io::Result<()> {
    let out = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("../openapi.yaml"));
    std::fs::write(&out, yevune_server::openapi::to_yaml())?;
    println!("已写出 OpenAPI 文档：{}", out.display());
    Ok(())
}
