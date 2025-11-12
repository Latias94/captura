use axum::{extract::Query, response::Response};
use axum_extra::typed_header::TypedHeader;
use headers::authorization::Bearer;
use headers::Authorization;
use serde::Deserialize;

use crate::auth::AuthUser;
use crate::error::{bad_request, internal, ApiResult};
use crate::AppState;

#[derive(Deserialize)]
pub(crate) struct MediaQuery {
    pub url: String,
}

pub(crate) async fn proxy(
    axum::extract::State(st): axum::extract::State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Query(q): Query<MediaQuery>,
) -> ApiResult<Response> {
    // 认证（校验 token 即可，不限制资源归属，后续可扩展策略）
    let _user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    // 校验 URL
    if q.url.len() > 2048 {
        return Err(bad_request("url too long"));
    }
    let parsed = url::Url::parse(&q.url).map_err(|_| bad_request("invalid url"))?;
    match parsed.scheme() {
        "http" | "https" => {}
        _ => return Err(bad_request("unsupported scheme")),
    }
    // 拉取资源（限时，避免占用过久）
    let cli = reqwest::Client::builder()
        .user_agent("captura-media-proxy/0.1")
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(internal)?;
    let resp = cli.get(parsed).send().await.map_err(internal)?;
    if !resp.status().is_success() {
        return Err(bad_request(format!("upstream status {}", resp.status())));
    }
    let ct = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let bytes = resp.bytes().await.map_err(internal)?;
    let mut out = Response::builder()
        .status(axum::http::StatusCode::OK)
        .body(axum::body::Body::from(bytes))
        .map_err(internal)?;
    if let Some(ct) = ct.as_deref() {
        out.headers_mut().insert(
            axum::http::header::CONTENT_TYPE,
            axum::http::HeaderValue::from_str(ct).unwrap_or(axum::http::HeaderValue::from_static(
                "application/octet-stream",
            )),
        );
    }
    Ok(out)
}
