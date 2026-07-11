//! 由 `contract` DTO + API 端点生成 OpenAPI 文档（设计文档 §11 / 计划 T10）。
//!
//! 组件 schema 直接取自 [`contract`] 的共享类型（`ToSchema` 派生），保证服务端与 web 端
//! 类型同源；路径覆盖 OpenSubsonic 兼容子集与自研扩展的主要端点。产物 `openapi.yaml`
//! 由 `gen_openapi` 二进制写出，并有测试守护其与本文档一致（防漂移）。

use utoipa::openapi::path::{HttpMethod, OperationBuilder, PathItemBuilder};
use utoipa::openapi::{OpenApi, ResponseBuilder};
use utoipa::OpenApi as _;

/// 由 contract DTO 派生组件 schema 的文档骨架。
#[derive(utoipa::OpenApi)]
#[openapi(
    info(
        title = "自托管家庭音乐服务端 API",
        version = "0.1.0",
        description = "OpenSubsonic 兼容子集 + 命名空间隔离的自研扩展（/rest/ext/*）。"
    ),
    components(schemas(
        contract::Genre,
        contract::Artist,
        contract::Album,
        contract::Track,
        contract::Playlist,
        contract::PlaylistFolder,
        contract::User,
        contract::Role,
        contract::Principal,
        contract::PrincipalType,
        contract::ScopeType,
        contract::AccessRule,
        contract::StreamRequest,
        contract::SubsonicError,
        contract::ResponseStatus,
    ))
)]
struct ApiDoc;

/// 覆盖的主要端点：OpenSubsonic 兼容子集 + 自研扩展代表端点。
/// 与 `api` 路由树对应；列表只描述能力，实际方法以路由实现为准。
const ENDPOINTS: &[(&str, &str)] = &[
    ("/rest/ping", "存活探测"),
    ("/rest/getLicense", "许可信息"),
    ("/rest/getOpenSubsonicExtensions", "声明自研扩展"),
    ("/rest/getArtists", "浏览艺人（按访问控制过滤）"),
    ("/rest/getArtist", "取单艺人及其专辑"),
    ("/rest/getAlbum", "取单专辑及其曲目"),
    ("/rest/getSong", "取单曲目"),
    ("/rest/getAlbumList2", "专辑列表"),
    ("/rest/getGenres", "流派列表"),
    ("/rest/getIndexes", "艺人索引"),
    ("/rest/search3", "全文搜索（FTS5）"),
    ("/rest/getPlaylists", "当前用户歌单（扁平）"),
    ("/rest/getPlaylist", "取单歌单及展开曲目"),
    ("/rest/createPlaylist", "创建歌单"),
    ("/rest/updatePlaylist", "更新歌单"),
    ("/rest/deletePlaylist", "删除歌单"),
    ("/rest/stream", "按需转码流（透传或转码）"),
    ("/rest/download", "下载原始文件"),
    ("/rest/getCoverArt", "封面图"),
    ("/rest/star", "收藏"),
    ("/rest/unstar", "取消收藏"),
    ("/rest/setRating", "评分"),
    ("/rest/scrobble", "播放上报"),
    ("/rest/getScanStatus", "扫描状态"),
    ("/rest/startScan", "触发扫描"),
    ("/rest/getUser", "取用户"),
    ("/rest/getUsers", "列用户"),
    ("/rest/createUser", "建用户"),
    ("/rest/updateUser", "改用户"),
    ("/rest/deleteUser", "删用户"),
    ("/rest/changePassword", "改密码"),
    ("/rest/ext/getPlaylistTree", "多级歌单树（扩展）"),
    ("/rest/ext/uploadTrack", "上传曲目入库（扩展，multipart）"),
    ("/rest/ext/setCoverArt", "替换专辑封面（扩展，multipart）"),
    (
        "/rest/ext/setAccessRule",
        "设置曲库访问规则（扩展，仅管理员）",
    ),
    ("/rest/ext/getAccessRules", "查询访问规则（扩展，仅管理员）"),
    ("/rest/ext/getRoles", "角色列表（扩展，仅管理员）"),
];

/// 构建完整 OpenAPI 文档：contract 组件 schema + 主要端点路径。
pub fn document() -> OpenApi {
    let mut doc = ApiDoc::openapi();
    for (path, summary) in ENDPOINTS {
        let operation = OperationBuilder::new()
            .summary(Some((*summary).to_string()))
            .response(
                "200",
                ResponseBuilder::new()
                    .description("subsonic-response 信封（XML 默认，`f=json` 返回 JSON）")
                    .build(),
            )
            .build();
        let item = PathItemBuilder::new()
            .operation(HttpMethod::Get, operation)
            .build();
        doc.paths.paths.insert((*path).to_string(), item);
    }
    doc
}

/// 序列化为 OpenAPI YAML 文本。
pub fn to_yaml() -> String {
    document().to_yaml().expect("OpenAPI 文档应可序列化为 YAML")
}
