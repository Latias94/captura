use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};

/// Route-level feature flags, inspired by RSSHub's `features` block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureConfig {
    pub name: &'static str,
    pub description: &'static str,
    pub optional: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Features {
    pub require_config: &'static [FeatureConfig],
    pub require_puppeteer: bool,
    pub anti_crawler: bool,
    pub support_bt: bool,
    pub support_podcast: bool,
    pub support_scihub: bool,
    pub nsfw: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Radar {
    pub source: &'static [&'static str],
    pub target: &'static str,
}

/// Parameter metadata used by Hub routes.
#[derive(Debug, Clone, Serialize)]
pub struct ParamMeta {
    pub name: &'static str,
    pub description: &'static str,
    pub default: Option<&'static str>,
    pub options: &'static [(&'static str, &'static str)],
}

/// Static route metadata (path/categories/parameters/etc.).
#[derive(Debug, Clone, Serialize)]
pub struct RouteMeta {
    /// Logical Hub id, e.g. "github/trending".
    pub hub_id: &'static str,
    /// Logical Hub path, e.g. "/github/trending/:since/:language".
    pub path: &'static str,
    pub categories: &'static [&'static str],
    pub example: &'static str,
    /// Parameter docs.
    pub params: &'static [ParamMeta],
    pub features: Features,
    pub radar: &'static [Radar],
    pub name: &'static str,
    pub maintainers: &'static [&'static str],
    pub url: &'static str,
    pub description: &'static str,
}

/// Item-level structure returned by Hub handlers (similar to RSSHub's DataItem).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubItem {
    pub title: String,
    pub description: Option<String>,
    pub link: Option<String>,
    pub author: Option<String>,
    pub pub_date: Option<DateTime<FixedOffset>>,
    pub categories: Vec<String>,
}

/// Route-level data returned by Hub handlers (similar to RSSHub's Data).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubData {
    pub title: String,
    pub description: Option<String>,
    pub link: Option<String>,
    pub image: Option<String>,
    pub language: Option<String>,
    pub items: Vec<HubItem>,
    pub allow_empty: bool,
}

/// Execution context passed to Hub handlers.
#[derive(Debug)]
pub struct HubCtx<'a> {
    pub hub_id: &'a str,
    /// Route parameters (path/query merged as needed by the dispatcher).
    pub params: &'a serde_json::Map<String, serde_json::Value>,
}

impl<'a> HubCtx<'a> {
    pub fn param_str(&self, key: &str) -> Option<&str> {
        self.params.get(key).and_then(|v| v.as_str())
    }

    /// Try to read a parameter as i64.
    ///
    /// Accepts both JSON numbers and string-encoded integers.
    pub fn param_i64(&self, key: &str) -> Option<i64> {
        if let Some(v) = self.params.get(key) {
            if let Some(n) = v.as_i64() {
                return Some(n);
            }
            if let Some(s) = v.as_str() {
                return s.parse().ok();
            }
        }
        None
    }
}

/// Future type returned by Hub handlers.
pub type HubHandlerFuture<'a> = std::pin::Pin<
    Box<dyn std::future::Future<Output = captura_common::Result<HubData>> + Send + 'a>,
>;

/// Simple function pointer type used by Hub routes (async handler).
pub type HubHandlerFn = for<'a> fn(&'a mut HubCtx<'a>) -> HubHandlerFuture<'a>;

/// A complete route definition: static meta + handler function.
#[derive(Clone, Copy)]
pub struct Route {
    pub meta: &'static RouteMeta,
    pub handler: HubHandlerFn,
}

/// Wrapper type used for `inventory`-based route registration.
#[derive(Clone, Copy)]
pub struct RouteWrapper(pub Route);

/// Backwards-compatibility alias for older code.
pub use HubCtx as HandlerCtx;
