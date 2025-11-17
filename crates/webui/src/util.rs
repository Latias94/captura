use axum::http::HeaderMap;
use base64::Engine as _;
use rand_core::{OsRng, RngCore};
use std::time::Duration;

#[derive(Default)]
pub struct UiSnippets {
    pub custom_css: String,
    pub custom_js: String,
    pub external_font_hosts: String,
}

/// Build a reqwest client for WebUI → API calls.
///
/// Behaviour:
/// - Prefer `captura-net` so that UA/timeout/proxy env settings are honoured;
/// - Fall back to a plain `reqwest::Client::builder()` with the same timeout
///   if `captura-net` configuration fails for any reason.
pub fn http_client(timeout_secs: u64) -> Option<reqwest::Client> {
    let timeout_ms = timeout_secs.saturating_mul(1000);
    if let Ok(cli) = captura_net::client_basic(None, Some(timeout_ms)) {
        return Some(cli);
    }
    reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .build()
        .ok()
}

/// Generate a CSP nonce suitable for script/style tags.
pub fn gen_csp_nonce() -> String {
    let mut buf = [0u8; 16];
    OsRng.fill_bytes(&mut buf);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf)
}

/// Load per-user UI snippets (custom CSS/JS and external font hosts) from `/v1/me`.
pub async fn load_snippets(headers: &HeaderMap) -> UiSnippets {
    let Some(token) = cookie_value(headers, "X-Auth-Token") else {
        return UiSnippets::default();
    };
    let Some(cli) = http_client(3) else {
        return UiSnippets::default();
    };
    let me = format!("{}/v1/me", api_base());
    if let Ok(resp) = cli
        .get(me)
        .header("X-Auth-Token", token)
        .send()
        .await
        .and_then(|r| r.error_for_status())
    {
        #[derive(serde::Deserialize)]
        struct Me {
            stylesheet: Option<String>,
            custom_js: Option<String>,
            external_font_hosts: Option<String>,
        }
        if let Ok(m) = resp.json::<Me>().await {
            return UiSnippets {
                custom_css: m.stylesheet.unwrap_or_default(),
                custom_js: m.custom_js.unwrap_or_default(),
                external_font_hosts: m.external_font_hosts.unwrap_or_default(),
            };
        }
    }
    UiSnippets::default()
}

/// Read the `X-Auth-Token` cookie from headers.
pub fn read_token_cookie(headers: &HeaderMap) -> Option<String> {
    let cookies = headers.get(axum::http::header::COOKIE)?.to_str().ok()?;
    for part in cookies.split(';') {
        let kv = part.trim();
        if let Some((k, v)) = kv.split_once('=') {
            if k.trim() == "X-Auth-Token" {
                return Some(v.trim().to_string());
            }
        }
    }
    None
}

/// Base API URL used by the WebUI when calling the backend.
pub fn api_base() -> String {
    std::env::var("CAPTURA_WEBUI_API_BASE")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "http://127.0.0.1:8080".into())
}

/// Read an arbitrary cookie value by name.
pub fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    let cookies = headers.get(axum::http::header::COOKIE)?.to_str().ok()?;
    for part in cookies.split(';') {
        let kv = part.trim();
        if let Some((k, v)) = kv.split_once('=') {
            if k.trim() == name {
                return Some(v.trim().to_string());
            }
        }
    }
    None
}

/// Resolve UI language from cookie, `/v1/me` prefs or Accept-Language.
pub async fn resolve_lang(headers: &HeaderMap) -> String {
    if let Some(lang) = cookie_value(headers, "lang") {
        return lang;
    }
    if let Some(token) = cookie_value(headers, "X-Auth-Token") {
        if let Some(cli) = http_client(2) {
            let me = format!("{}/v1/me", api_base());
            if let Ok(resp) = cli
                .get(me)
                .header("X-Auth-Token", token)
                .send()
                .await
                .and_then(|r| r.error_for_status())
            {
                #[derive(serde::Deserialize)]
                struct Me {
                    language: Option<String>,
                }
                if let Ok(m) = resp.json::<Me>().await {
                    if let Some(l) = m.language {
                        return l;
                    }
                }
            }
        }
    }
    // Accept-Language heuristic
    if let Some(al) = headers
        .get(axum::http::header::ACCEPT_LANGUAGE)
        .and_then(|v| v.to_str().ok())
    {
        let al = al.to_ascii_lowercase();
        if al.contains("zh") {
            return "zh_CN".into();
        }
    }
    "en_US".into()
}
