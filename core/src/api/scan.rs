use crate::{auth::AuthenticatedSession, error::Result, http::HttpClient};
use serde::Deserialize;

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
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Payload {
    scan_status: ScanStatus,
}
