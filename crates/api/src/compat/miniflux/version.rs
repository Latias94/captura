use super::error::MfResult;
use crate::AppState;
use axum::Json;
use axum::extract::State;

#[derive(serde::Serialize)]
pub(crate) struct MfVersionResp {
    pub version: String,
    pub commit: String,
    pub build_date: String,
    pub go_version: String,
    pub compiler: String,
    pub arch: String,
    pub os: String,
}

pub(crate) async fn version(
    State(_st): State<AppState>,
    _headers: axum::http::HeaderMap,
) -> MfResult<Json<MfVersionResp>> {
    Ok(Json(MfVersionResp {
        version: env!("CARGO_PKG_VERSION").to_string(),
        commit: String::new(),
        build_date: String::new(),
        go_version: String::new(),
        compiler: String::from("rustc"),
        arch: std::env::consts::ARCH.to_string(),
        os: std::env::consts::OS.to_string(),
    }))
}
