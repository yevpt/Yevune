//! OpenSubsonic 响应信封：统一 `subsonic-response`，支持 XML（默认）与 `f=json`。
//!
//! handler 用一个 `serde_json::Value` 载荷（对象）描述响应体，本模块据此渲染为 JSON 或
//! 由通用 [`object_to_xml`] 转换器生成 OpenSubsonic 约定的 XML（标量→属性、数组/对象→子元素）。

use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{Map, Value};

/// 声明兼容的 OpenSubsonic/Subsonic 协议版本。
const API_VERSION: &str = "1.16.1";
/// 服务端标识（对应 `subsonic-response` 的 `type` 字段）。
const SERVER_TYPE: &str = "music-server";

/// 响应格式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// XML（OpenSubsonic 默认）。
    Xml,
    /// JSON（`f=json`）。
    Json,
}

impl Format {
    /// 由查询参数 `f` 判定；`json` → JSON，其余（含缺省）→ XML。
    pub fn from_param(f: Option<&str>) -> Self {
        match f {
            Some("json") => Format::Json,
            _ => Format::Xml,
        }
    }
}

/// 成功响应：把 `payload`（对象）并入 `subsonic-response` 信封。
pub fn ok(format: Format, payload: Value) -> Response {
    envelope(format, "ok", payload)
}

/// 错误响应：`subsonic-response` 携带 `error{code,message}`，HTTP 状态仍为 200（OpenSubsonic 约定）。
pub fn error(format: Format, code: u32, message: &str) -> Response {
    let mut payload = Map::new();
    let mut err = Map::new();
    err.insert("code".into(), Value::from(code));
    err.insert("message".into(), Value::from(message));
    payload.insert("error".into(), Value::Object(err));
    envelope(format, "failed", Value::Object(payload))
}

/// OpenSubsonic 「未找到」错误码（用于受限内容对无授权者的统一遮蔽）。
pub const ERROR_NOT_FOUND: u32 = 70;
/// OpenSubsonic 「参数缺失」错误码。
pub const ERROR_MISSING_PARAM: u32 = 10;

fn envelope(format: Format, status: &str, payload: Value) -> Response {
    let payload_obj = payload.as_object().cloned().unwrap_or_default();
    match format {
        Format::Json => {
            let mut resp = base_map(status);
            for (k, v) in payload_obj {
                resp.insert(k, v);
            }
            let mut root = Map::new();
            root.insert("subsonic-response".into(), Value::Object(resp));
            Json(Value::Object(root)).into_response()
        }
        Format::Xml => {
            let (attrs, children) = object_to_xml(&payload_obj);
            let body = format!(
                concat!(
                    r#"<?xml version="1.0" encoding="UTF-8"?>"#,
                    "\n",
                    r#"<subsonic-response xmlns="http://subsonic.org/restapi" status="{status}" "#,
                    r#"version="{version}" type="{type}" serverVersion="{server}" openSubsonic="true"{attrs}>"#,
                    "{children}</subsonic-response>",
                ),
                status = status,
                version = API_VERSION,
                r#type = SERVER_TYPE,
                server = env!("CARGO_PKG_VERSION"),
                attrs = attrs,
                children = children,
            );
            (
                [(header::CONTENT_TYPE, "application/xml; charset=utf-8")],
                body,
            )
                .into_response()
        }
    }
}

/// 信封固定字段（JSON）。
fn base_map(status: &str) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("status".into(), Value::from(status));
    m.insert("version".into(), Value::from(API_VERSION));
    m.insert("type".into(), Value::from(SERVER_TYPE));
    m.insert(
        "serverVersion".into(),
        Value::from(env!("CARGO_PKG_VERSION")),
    );
    m.insert("openSubsonic".into(), Value::from(true));
    m
}

/// 把一个 JSON 对象按 OpenSubsonic 约定拆成 (属性串, 子元素串)。
///
/// 标量（字符串/数字/布尔）→ 属性；对象 → 子元素；数组 → 同名子元素重复；`null` → 略过。
fn object_to_xml(obj: &Map<String, Value>) -> (String, String) {
    let mut attrs = String::new();
    let mut children = String::new();
    for (key, val) in obj {
        match val {
            Value::Null => {}
            Value::Bool(b) => attrs.push_str(&format!(" {key}=\"{b}\"")),
            Value::Number(n) => attrs.push_str(&format!(" {key}=\"{n}\"")),
            Value::String(s) => attrs.push_str(&format!(" {key}=\"{}\"", xml_escape(s))),
            Value::Array(items) => {
                for item in items {
                    children.push_str(&element(key, item));
                }
            }
            Value::Object(_) => children.push_str(&element(key, val)),
        }
    }
    (attrs, children)
}

/// 渲染单个命名元素。
fn element(name: &str, val: &Value) -> String {
    match val {
        Value::Object(o) => {
            let (attrs, children) = object_to_xml(o);
            if children.is_empty() {
                format!("<{name}{attrs}/>")
            } else {
                format!("<{name}{attrs}>{children}</{name}>")
            }
        }
        Value::String(s) => format!("<{name}>{}</{name}>", xml_escape(s)),
        Value::Number(n) => format!("<{name}>{n}</{name}>"),
        Value::Bool(b) => format!("<{name}>{b}</{name}>"),
        Value::Null => String::new(),
        Value::Array(items) => items.iter().map(|i| element(name, i)).collect(),
    }
}

/// 转义 XML 文本/属性中的特殊字符。
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
