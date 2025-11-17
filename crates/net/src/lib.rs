//! Shared networking + HTML helpers used across Captura.
//!
//! This crate centralizes:
//! - Basic reqwest client construction with env-based configuration;
//! - Small HTML helpers built on top of `scraper`.

use captura_common::{Error, Result};
use reqwest::Client;
use std::env;
use std::time::Duration;

pub mod html;

/// Build a simple HTTP client with optional user agent and timeout.
///
/// Behaviour:
/// - User-Agent: explicit override > `CAPTURA_HTTP_USER_AGENT` env > default `captura/0.1`.
/// - Timeout (ms): explicit override > `CAPTURA_HTTP_TIMEOUT_MS` env; 0 or invalid means no timeout.
/// - Proxy: optional `CAPTURA_HTTP_PROXY` URL applied to all requests.
pub fn client_basic(user_agent: Option<String>, timeout_ms: Option<u64>) -> Result<Client> {
    let mut builder = Client::builder();

    // User-Agent: explicit override > env > default.
    let ua = user_agent
        .or_else(|| {
            env::var("CAPTURA_HTTP_USER_AGENT")
                .ok()
                .filter(|s| !s.trim().is_empty())
        })
        .unwrap_or_else(|| "captura/0.1".to_string());
    builder = builder.user_agent(ua);

    // Timeout: explicit override > env (milliseconds).
    let effective_timeout = timeout_ms.or_else(|| {
        env::var("CAPTURA_HTTP_TIMEOUT_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
    });
    if let Some(ms) = effective_timeout {
        if ms > 0 {
            builder = builder.timeout(Duration::from_millis(ms));
        }
    }

    // Optional proxy for all HTTP calls using this client.
    if let Ok(proxy_url) = env::var("CAPTURA_HTTP_PROXY") {
        let proxy_url = proxy_url.trim();
        if !proxy_url.is_empty() {
            match reqwest::Proxy::all(proxy_url) {
                Ok(p) => {
                    builder = builder.proxy(p);
                }
                Err(e) => {
                    return Err(Error::Config(format!("invalid CAPTURA_HTTP_PROXY: {e}")));
                }
            }
        }
    }

    builder.build().map_err(|e| Error::Network(e.to_string()))
}
