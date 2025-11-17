use captura_common::{NormalizedEntry, Result};
use reqwest::Client;
use scraper::{Html, Selector};
use serde_json::Value as JsonValue;
use std::time::Duration;
use tracing::debug;

use crate::v1::{JsonMappingSpec, JsonSourceSpec, RuleSpecV1, SourceType};

/// HTTP execution context for rule evaluation (stateless, DB-agnostic).
#[derive(Debug, Clone, Default)]
pub struct RuleExecHttpCtx {
    pub user_agent: Option<String>,
    pub headers: Option<serde_json::Map<String, JsonValue>>,
    pub cookies: Option<String>,
    pub basic_auth: Option<(String, Option<String>)>,
    pub proxy_url: Option<String>,
    pub timeout_ms: Option<u64>,
    pub smart: Option<bool>,
    pub respect_robots: Option<bool>,
}

/// Rule execution context combining HTTP options and merged params.
#[derive(Debug, Clone, Default)]
pub struct RuleExecCtx {
    pub http: RuleExecHttpCtx,
    /// Merged parameters (rule defaults + caller overrides).
    pub params: Option<JsonValue>,
}

fn json_get_path<'a>(v: &'a JsonValue, path: &str) -> Option<&'a JsonValue> {
    if path.is_empty() {
        return Some(v);
    }
    let mut cur = v;
    for part in path.split('.') {
        match cur {
            JsonValue::Object(map) => {
                cur = map.get(part)?;
            }
            _ => return None,
        }
    }
    Some(cur)
}

fn map_json_items(items: &[JsonValue], mapping: &JsonMappingSpec) -> Vec<NormalizedEntry> {
    let mut entries = Vec::new();
    for item in items {
        let mut entry = NormalizedEntry {
            guid: None,
            url: None,
            title: None,
            summary: None,
            content_html: None,
            author: None,
            published_at: None,
            enclosures: Vec::new(),
            extras: serde_json::json!({}),
        };

        if let Some(path) = &mapping.title {
            if let Some(v) = json_get_path(item, path) {
                if let Some(s) = v.as_str() {
                    entry.title = Some(s.to_string());
                }
            }
        }
        if let Some(path) = &mapping.url {
            if let Some(v) = json_get_path(item, path) {
                if let Some(s) = v.as_str() {
                    entry.url = Some(s.to_string());
                    entry.guid = Some(s.to_string());
                }
            }
        }
        if let Some(path) = &mapping.summary {
            if let Some(v) = json_get_path(item, path) {
                if let Some(s) = v.as_str() {
                    entry.summary = Some(s.to_string());
                }
            }
        }
        if let Some(path) = &mapping.content_html {
            if let Some(v) = json_get_path(item, path) {
                if let Some(s) = v.as_str() {
                    entry.content_html = Some(s.to_string());
                }
            }
        }
        if let Some(path) = &mapping.author {
            if let Some(v) = json_get_path(item, path) {
                if let Some(s) = v.as_str() {
                    entry.author = Some(s.to_string());
                }
            }
        }

        // TODO: published_at / enclosure mapping can be added here as needed.

        entries.push(entry);
    }
    entries
}

fn extract_items_for_source(
    root: Option<&str>,
    value: &JsonValue,
    mapping: &JsonMappingSpec,
) -> Vec<NormalizedEntry> {
    match root {
        None => match value {
            JsonValue::Array(arr) => map_json_items(arr, mapping),
            JsonValue::Object(_) => map_json_items(std::slice::from_ref(value), mapping),
            _ => Vec::new(),
        },
        Some(path) => {
            // When the root is applied to an array (e.g. aggregated JSON
            // documents from `from_html.multiple=true`), walk each element and
            // collect items from the path inside that element.
            if let JsonValue::Array(arr) = value {
                let mut out = Vec::new();
                for elem in arr {
                    if let Some(target) = json_get_path(elem, path) {
                        match target {
                            JsonValue::Array(inner) => {
                                out.extend(map_json_items(inner, mapping));
                            }
                            JsonValue::Object(_) => {
                                out.extend(map_json_items(std::slice::from_ref(target), mapping));
                            }
                            _ => {}
                        }
                    }
                }
                out
            } else {
                let base = match json_get_path(value, path) {
                    Some(v) => v,
                    None => return Vec::new(),
                };
                match base {
                    JsonValue::Array(arr) => map_json_items(arr, mapping),
                    JsonValue::Object(_) => map_json_items(std::slice::from_ref(base), mapping),
                    _ => Vec::new(),
                }
            }
        }
    }
}

/// Stateless execution of `source.type = json` rules.
///
/// Currently covers only JSON rules; suitable as the core engine for Hub routes
/// or environments without direct access to database models.
pub async fn execute_json_v1_stateless(
    spec: &RuleSpecV1,
    ctx: &RuleExecCtx,
) -> Result<Vec<NormalizedEntry>> {
    if !matches!(spec.source.kind, SourceType::Json) {
        return Err(captura_common::Error::Config(
            "execute_json_v1_stateless: source.type != json".into(),
        ));
    }

    let ua = spec
        .fetch
        .user_agent
        .clone()
        .or_else(|| ctx.http.user_agent.clone())
        .unwrap_or_else(|| "captura/0.1".to_string());

    let mut client_builder = Client::builder().user_agent(ua);
    if let Some(proxy) = &ctx.http.proxy_url {
        if !proxy.is_empty() {
            if let Ok(p) = reqwest::Proxy::all(proxy) {
                client_builder = client_builder.proxy(p);
            }
        }
    }
    let client = client_builder
        .build()
        .map_err(|e| captura_common::Error::Network(e.to_string()))?;

    let mut entries = Vec::new();

    // Multi-source mode: source.sources present.
    if let Some(sources) = &spec.source.sources {
        for src in sources {
            let src_entries = fetch_and_map_json_source(&client, spec, ctx, src).await?;
            entries.extend(src_entries);
        }
    } else {
        // Single-source mode: use source.request/root/mapping or from_html.
        let mapping = spec.source.mapping.as_ref().ok_or_else(|| {
            captura_common::Error::Config("source.mapping is required for json".into())
        })?;

        if let Some(from_html) = &spec.source.from_html {
            // JSON embedded in HTML: fetch HTML, extract JSON text from nodes
            // selected by CSS, then parse and apply root/mapping.
            let req = from_html
                .request
                .as_ref()
                .or(spec.source.request.as_ref())
                .ok_or_else(|| {
                    captura_common::Error::Config(
                        "source.request or from_html.request is required for json/from_html".into(),
                    )
                })?;

            let html_text = fetch_json_text(&client, spec, ctx, req).await?;
            let doc = Html::parse_document(&html_text);
            let selector = Selector::parse(&from_html.selector).map_err(|e| {
                captura_common::Error::Config(format!("invalid from_html.selector: {e}"))
            })?;

            let mut docs: Vec<JsonValue> = Vec::new();
            for node in doc.select(&selector) {
                let text = node.text().collect::<String>();
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    continue;
                }
                match serde_json::from_str::<JsonValue>(trimmed) {
                    Ok(v) => docs.push(v),
                    Err(e) => {
                        debug!("failed to parse JSON from from_html node: {}", e);
                    }
                }
            }

            if !docs.is_empty() {
                let root = spec.source.root.as_deref();
                let value = if from_html.multiple.unwrap_or(false) {
                    JsonValue::Array(docs)
                } else {
                    // NOTE: we already checked `docs` is non-empty.
                    docs.into_iter().next().unwrap()
                };
                let mapped = extract_items_for_source(root, &value, mapping);
                entries.extend(mapped);
            }
        } else {
            // Pure JSON source: fetch and map as before.
            let req = spec.source.request.as_ref().ok_or_else(|| {
                captura_common::Error::Config("source.request is required for json".into())
            })?;
            let html_or_json = fetch_json_text(&client, spec, ctx, req).await?;
            let value: JsonValue = serde_json::from_str(&html_or_json)
                .map_err(|e| captura_common::Error::Parse(e.to_string()))?;

            let root = spec.source.root.as_deref();
            let mapped = extract_items_for_source(root, &value, mapping);
            entries.extend(mapped);
        }
    }

    Ok(entries)
}

async fn fetch_and_map_json_source(
    client: &Client,
    spec: &RuleSpecV1,
    ctx: &RuleExecCtx,
    src: &JsonSourceSpec,
) -> Result<Vec<NormalizedEntry>> {
    let req = &src.request;
    let html_or_json = fetch_json_text(client, spec, ctx, req).await?;
    let value: JsonValue = serde_json::from_str(&html_or_json)
        .map_err(|e| captura_common::Error::Parse(e.to_string()))?;
    let mapped = extract_items_for_source(src.root.as_deref(), &value, &src.mapping);
    Ok(mapped)
}

async fn fetch_json_text(
    client: &Client,
    spec: &RuleSpecV1,
    ctx: &RuleExecCtx,
    req: &crate::v1::RequestSpec,
) -> Result<String> {
    let params = ctx.params.as_ref();
    let final_url = render_with_params(&req.url, params);

    let mut http_req = match req
        .method
        .as_deref()
        .unwrap_or("GET")
        .to_ascii_uppercase()
        .as_str()
    {
        "POST" => client.post(&final_url),
        "PUT" => client.put(&final_url),
        "DELETE" => client.delete(&final_url),
        _ => client.get(&final_url),
    };

    // Apply headers from request + ctx.
    if let Some(headers) = &req.headers {
        for (k, v) in headers {
            if let Some(s) = v.as_str() {
                if let Ok(name) = reqwest::header::HeaderName::from_bytes(k.as_bytes()) {
                    if let Ok(val) = reqwest::header::HeaderValue::from_str(s) {
                        http_req = http_req.header(name, val);
                    }
                }
            }
        }
    }
    if let Some(headers) = &ctx.http.headers {
        for (k, v) in headers {
            if let Some(s) = v.as_str() {
                if let Ok(name) = reqwest::header::HeaderName::from_bytes(k.as_bytes()) {
                    if let Ok(val) = reqwest::header::HeaderValue::from_str(s) {
                        http_req = http_req.header(name, val);
                    }
                }
            }
        }
    }

    // Timeout from request or spec-level fetch.
    if let Some(ms) = req
        .timeout_ms
        .or(spec.fetch.timeout_ms)
        .or(ctx.http.timeout_ms)
    {
        http_req = http_req.timeout(Duration::from_millis(ms));
    }

    if let Some(ref c) = ctx.http.cookies {
        if !c.is_empty() {
            http_req = http_req.header(reqwest::header::COOKIE, c.clone());
        }
    }
    if let Some((ref u, ref p)) = ctx.http.basic_auth {
        http_req = http_req.basic_auth(u, p.clone());
    }

    v1_exec_send(http_req, &final_url).await
}

async fn v1_exec_send(req: reqwest::RequestBuilder, url: &str) -> Result<String> {
    let resp = req
        .send()
        .await
        .map_err(|e| captura_common::Error::Network(format!("{} -> {}", url, e)))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(captura_common::Error::Network(format!(
            "{} -> http status {}",
            url, status
        )));
    }
    let text = resp
        .text()
        .await
        .map_err(|e| captura_common::Error::Network(e.to_string()))?;
    debug!(url = url, "execute_json_v1_stateless: fetched json");
    Ok(text)
}

/// Very simple template renderer for URLs like `...?limit={limit}&platform={platform}`.
fn render_with_params(template: &str, params: Option<&JsonValue>) -> String {
    let mut out = template.to_string();
    if let Some(JsonValue::Object(map)) = params {
        for (k, v) in map {
            let placeholder = format!("{{{}}}", k);
            let val = if let Some(s) = v.as_str() {
                s.to_string()
            } else {
                v.to_string()
            };
            out = out.replace(&placeholder, &val);
        }
    }
    out
}
