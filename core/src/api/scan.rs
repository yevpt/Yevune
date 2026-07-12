use crate::{auth::AuthenticatedSession, error::Result, http::HttpClient};
use serde::Deserialize;

#[derive(Clone, Deserialize, uniffi::Enum)]
#[serde(rename_all = "lowercase")]
pub enum ScanAction {
    Added,
    Updated,
    Deleted,
}

#[derive(Clone, Deserialize, uniffi::Record)]
#[serde(rename_all = "camelCase")]
pub struct ScanChange {
    pub action: ScanAction,
    pub object_key: String,
    pub track: contract::Track,
}

#[derive(Clone, Deserialize, uniffi::Record)]
#[serde(rename_all = "camelCase")]
pub struct DetailedScanResult {
    pub added: u32,
    pub updated: u32,
    pub deleted: u32,
    pub unchanged: u32,
    pub changes: Vec<ScanChange>,
    pub changes_truncated: bool,
}

#[derive(Clone, Deserialize, uniffi::Record)]
pub struct ScanStatus {
    pub scanning: bool,
    pub count: u32,
}

pub(crate) async fn start(http: &HttpClient, auth: &AuthenticatedSession) -> Result<ScanStatus> {
    let payload: Payload = http.get_json(auth, "startScan", &[]).await?;
    Ok(payload.scan_status)
}
pub(crate) async fn status(http: &HttpClient, auth: &AuthenticatedSession) -> Result<ScanStatus> {
    let payload: Payload = http.get_json(auth, "getScanStatus", &[]).await?;
    Ok(payload.scan_status)
}

pub(crate) async fn prefix(
    http: &HttpClient,
    auth: &AuthenticatedSession,
    prefix: String,
) -> Result<DetailedScanResult> {
    let payload: DetailedPayload = http
        .get_json(auth, "ext/startScan", &[("prefix".into(), prefix)])
        .await?;
    Ok(payload.scan_result)
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Payload {
    scan_status: ScanStatus,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DetailedPayload {
    scan_result: DetailedScanResult,
}
