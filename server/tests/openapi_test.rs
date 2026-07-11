//! OpenAPI 生成：覆盖主要端点与 contract 组件 schema，且产物与实现一致（计划 T10）。

use music_server::openapi::{document, to_yaml};

#[test]
fn 文档覆盖主要端点() {
    let doc = document();
    for path in [
        "/rest/ping",
        "/rest/getArtists",
        "/rest/getAlbum",
        "/rest/getSong",
        "/rest/getAlbumList2",
        "/rest/search3",
        "/rest/stream",
        "/rest/download",
        "/rest/getCoverArt",
        "/rest/getPlaylists",
        "/rest/ext/setAccessRule",
    ] {
        assert!(doc.paths.paths.contains_key(path), "缺端点 {path}");
    }
}

#[test]
fn 文档含_contract_组件_schema() {
    let doc = document();
    let schemas = doc
        .components
        .as_ref()
        .expect("应有 components")
        .schemas
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    for name in ["Album", "Track", "Artist", "User", "Playlist", "AccessRule"] {
        assert!(schemas.contains(&name.to_string()), "缺 schema {name}");
    }
}

#[test]
fn yaml_非空且含关键内容() {
    let yaml = to_yaml();
    assert!(yaml.len() > 500, "OpenAPI YAML 不应为空骨架");
    assert!(yaml.contains("openapi:"));
    assert!(yaml.contains("/rest/stream"));
    // camelCase 字段名须与 serde 序列化一致，保证 web 端 TS 类型同源。
    assert!(yaml.contains("bitRate"));
}

#[test]
fn 产物_openapi_yaml_与实现一致() {
    let committed =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../openapi.yaml"))
            .expect("仓库根应有 openapi.yaml；用 `cargo run --bin gen_openapi` 生成");
    assert_eq!(
        committed,
        to_yaml(),
        "openapi.yaml 与实现漂移；请运行 `cargo run --bin gen_openapi` 重新生成"
    );
}
