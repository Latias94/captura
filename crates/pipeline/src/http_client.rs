use captura_common::{Error, Result};
use captura_storage::entity::feed;
use reqwest::Client;
use std::env;
use std::time::Duration;

/// Build an HTTP client using feed-level settings with optional overrides.
///
/// - `user_agent_override`: if `Some`, overrides `feed.user_agent`.
/// - `timeout_ms_override`: if `Some`, overrides `feed.request_timeout_ms`.
pub(crate) fn client_for_feed(
    feed: &feed::Model,
    user_agent_override: Option<String>,
    timeout_ms_override: Option<u64>,
) -> Result<Client> {
    let mut builder = Client::builder();

    // User agent: override > feed-level UA.
    let ua = user_agent_override.or_else(|| feed.user_agent.clone());
    if let Some(ua) = ua {
        builder = builder.user_agent(ua);
    }

    // Timeout: override > feed.request_timeout_ms (if positive).
    let timeout_ms = timeout_ms_override.or_else(|| feed.request_timeout_ms.map(|v| v as u64));
    if let Some(ms) = timeout_ms {
        if ms > 0 {
            builder = builder.timeout(Duration::from_millis(ms));
        }
    }

    // TLS / HTTP/2 flags.
    if feed.allow_invalid_certs {
        builder = builder.danger_accept_invalid_certs(true);
    }
    if feed.disable_http2 {
        builder = builder.http1_only();
    }

    // Proxy support (when enabled on the feed).
    if feed.fetch_via_proxy {
        if let Some(ref p) = feed.proxy_url {
            if !p.is_empty() {
                if let Ok(proxy) = reqwest::Proxy::all(p) {
                    builder = builder.proxy(proxy);
                }
            }
        }
    }

    builder.build().map_err(|e| Error::Network(e.to_string()))
}

/// Build a simple HTTP client with optional user agent and timeout.
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

    // Optional proxy for all Hub/pipeline HTTP calls.
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
