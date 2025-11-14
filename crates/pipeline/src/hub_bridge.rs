use captura_common::{NormalizedEntry, Result};
use captura_hub::types::{HandlerCtx as HubHandlerCtx, HubData, HubHandler, HubItem, HubResult};
use captura_rules::v1::RuleSpecV1;
use captura_storage::entity::feed;
use tracing::debug;

use crate::hub_utils;

struct BuiltinHubRoute {
    hub_id: &'static str,
    handler: &'static dyn HubHandler,
}

static BUILTIN_HUB_ROUTES: &[BuiltinHubRoute] = &[
    BuiltinHubRoute {
        hub_id: "github/trending",
        handler: &GITHUB_TRENDING_HANDLER,
    },
    BuiltinHubRoute {
        hub_id: "hn/front",
        handler: &HN_FRONT_HANDLER,
    },
    BuiltinHubRoute {
        hub_id: "lobsters/front",
        handler: &LOBSTERS_FRONT_HANDLER,
    },
    BuiltinHubRoute {
        hub_id: "zhihu/hotlist",
        handler: &ZHIHU_HOTLIST_HANDLER,
    },
    BuiltinHubRoute {
        hub_id: "reuters/top",
        handler: &REUTERS_TOP_HANDLER,
    },
    BuiltinHubRoute {
        hub_id: "medium/tag",
        handler: &MEDIUM_TAG_HANDLER,
    },
];

fn find_builtin_handler(hub_id: &str) -> Option<&'static dyn HubHandler> {
    BUILTIN_HUB_ROUTES
        .iter()
        .find(|r| r.hub_id == hub_id)
        .map(|r| r.handler)
}

/// Built-in Hub handler for github/trending.
pub struct GithubTrendingHubHandler;

#[async_trait::async_trait]
impl HubHandler for GithubTrendingHubHandler {
    async fn handle(&self, ctx: &mut HubHandlerCtx<'_>) -> captura_common::Result<HubResult> {
        let since = ctx.param_str("since").unwrap_or("daily");
        let language = ctx.param_str("language").unwrap_or("");
        let spoken = ctx.param_str("spoken_language").unwrap_or("");

        let mut url = if language.is_empty() || language == "any" {
            "https://github.com/trending".to_string()
        } else {
            format!("https://github.com/trending/{}", language)
        };
        let mut qs = vec![format!("since={}", since)];
        if !spoken.is_empty() {
            qs.push(format!("spoken_language_code={}", spoken));
        }
        if !qs.is_empty() {
            url.push('?');
            url.push_str(&qs.join("&"));
        }

        let mut items: Vec<HubItem> = Vec::new();
        let opts = hub_utils::HubHttpOpts::default();
        let html = hub_utils::get_html(&url, &opts, None).await?;

        hub_utils::for_each_element(&html, "article.Box-row", |el| {
            let link = crate::extract_attr(&el, "h2 a@href")
                .map(|href| hub_utils::absolutize(&url, &href));
            let title = crate::extract_text(&el, "h2 a");
            let desc_html = hub_utils::element_html_sanitized(&el);
            items.push(HubItem {
                title: title.unwrap_or_else(|| link.clone().unwrap_or_default()),
                description: Some(desc_html),
                link,
                author: None,
                pub_date: None,
                categories: Vec::new(),
            });
        })?;

        let data = HubData {
            title: "GitHub Trending".to_string(),
            description: Some("GitHub trending repositories".to_string()),
            link: Some("https://github.com/trending".to_string()),
            image: None,
            language: None,
            items,
            allow_empty: false,
        };

        debug!(
            hub_id = ctx.hub_id,
            items = data.items.len(),
            "github_trending hub handler"
        );

        Ok(HubResult::Data(data))
    }
}

static GITHUB_TRENDING_HANDLER: GithubTrendingHubHandler = GithubTrendingHubHandler;

/// Built-in Hub handler for hn/front (Hacker News front page).
pub struct HnFrontHubHandler;

#[async_trait::async_trait]
impl HubHandler for HnFrontHubHandler {
    async fn handle(&self, ctx: &mut HubHandlerCtx<'_>) -> captura_common::Result<HubResult> {
        let url = "https://news.ycombinator.com/".to_string();

        let mut items: Vec<HubItem> = Vec::new();
        let opts = hub_utils::HubHttpOpts::default();
        let html = hub_utils::get_html(&url, &opts, None).await?;

        hub_utils::for_each_element(&html, "tr.athing", |el| {
            let link = crate::extract_attr(&el, "span.titleline a@href")
                .map(|href| hub_utils::absolutize(&url, &href));
            let title = crate::extract_text(&el, "span.titleline a");
            let desc_html = hub_utils::element_html_sanitized(&el);
            items.push(HubItem {
                title: title
                    .clone()
                    .unwrap_or_else(|| link.clone().unwrap_or_default()),
                description: Some(desc_html),
                link,
                author: None,
                pub_date: None,
                categories: Vec::new(),
            });
        })?;

        let data = HubData {
            title: "Hacker News Front Page".to_string(),
            description: Some("Hacker News front page stories.".to_string()),
            link: Some("https://news.ycombinator.com/".to_string()),
            image: None,
            language: None,
            items,
            allow_empty: false,
        };

        debug!(
            hub_id = ctx.hub_id,
            items = data.items.len(),
            "hn_front hub handler"
        );

        Ok(HubResult::Data(data))
    }
}

static HN_FRONT_HANDLER: HnFrontHubHandler = HnFrontHubHandler;

/// Built-in Hub handler for lobsters/front.
pub struct LobstersFrontHubHandler;

#[async_trait::async_trait]
impl HubHandler for LobstersFrontHubHandler {
    async fn handle(&self, ctx: &mut HubHandlerCtx<'_>) -> captura_common::Result<HubResult> {
        let url = "https://lobste.rs/".to_string();

        let mut items: Vec<HubItem> = Vec::new();
        let opts = hub_utils::HubHttpOpts::default();
        let html = hub_utils::get_html(&url, &opts, None).await?;

        hub_utils::for_each_element(&html, "li.story", |el| {
            let link = crate::extract_attr(&el, "h2 a@href")
                .map(|href| hub_utils::absolutize(&url, &href));
            let title = crate::extract_text(&el, "h2 a");
            let desc_html = hub_utils::element_html_sanitized(&el);
            items.push(HubItem {
                title: title.unwrap_or_else(|| link.clone().unwrap_or_default()),
                description: Some(desc_html),
                link,
                author: None,
                pub_date: None,
                categories: Vec::new(),
            });
        })?;

        let data = HubData {
            title: "Lobsters Front Page".to_string(),
            description: Some("Lobsters front page stories.".to_string()),
            link: Some("https://lobste.rs/".to_string()),
            image: None,
            language: None,
            items,
            allow_empty: false,
        };

        debug!(
            hub_id = ctx.hub_id,
            items = data.items.len(),
            "lobsters_front hub handler"
        );

        Ok(HubResult::Data(data))
    }
}

static LOBSTERS_FRONT_HANDLER: LobstersFrontHubHandler = LobstersFrontHubHandler;

/// Built-in Hub handler for zhihu/hotlist.
pub struct ZhihuHotlistHubHandler;

#[async_trait::async_trait]
impl HubHandler for ZhihuHotlistHubHandler {
    async fn handle(&self, ctx: &mut HubHandlerCtx<'_>) -> captura_common::Result<HubResult> {
        let url = "https://www.zhihu.com/hot".to_string();

        let mut items: Vec<HubItem> = Vec::new();
        let mut opts = hub_utils::HubHttpOpts::default();
        // Zhihu 反爬较严格，后续可在此扩展 UA/headers。
        opts.smart = false;
        let html = hub_utils::get_html(&url, &opts, None).await?;

        hub_utils::for_each_element(&html, "div.HotItem", |el| {
            let link = crate::extract_attr(&el, "a.HotItem-title@href")
                .map(|href| hub_utils::absolutize(&url, &href));
            let title = crate::extract_text(&el, "a.HotItem-title");
            let desc_html = hub_utils::element_html_sanitized(&el);
            items.push(HubItem {
                title: title.unwrap_or_else(|| link.clone().unwrap_or_default()),
                description: Some(desc_html),
                link,
                author: None,
                pub_date: None,
                categories: Vec::new(),
            });
        })?;

        let data = HubData {
            title: "Zhihu Hot List".to_string(),
            description: Some("Zhihu hot list entries.".to_string()),
            link: Some("https://www.zhihu.com/hot".to_string()),
            image: None,
            language: None,
            items,
            allow_empty: false,
        };

        debug!(
            hub_id = ctx.hub_id,
            items = data.items.len(),
            "zhihu_hotlist hub handler"
        );

        Ok(HubResult::Data(data))
    }
}

static ZHIHU_HOTLIST_HANDLER: ZhihuHotlistHubHandler = ZhihuHotlistHubHandler;

/// Built-in Hub handler for reuters/top.
pub struct ReutersTopHubHandler;

#[async_trait::async_trait]
impl HubHandler for ReutersTopHubHandler {
    async fn handle(&self, ctx: &mut HubHandlerCtx<'_>) -> captura_common::Result<HubResult> {
        let url = "https://www.reuters.com/world/".to_string();

        let mut items: Vec<HubItem> = Vec::new();
        let opts = hub_utils::HubHttpOpts::default();
        let html = hub_utils::get_html(&url, &opts, None).await?;

        hub_utils::for_each_element(&html, "article.story-card, article.story", |el| {
            let link =
                crate::extract_attr(&el, "a@href").map(|href| hub_utils::absolutize(&url, &href));
            let title = crate::extract_text(&el, "h3").or_else(|| crate::extract_text(&el, "h2"));
            let desc_html = hub_utils::element_html_sanitized(&el);
            items.push(HubItem {
                title: title.unwrap_or_else(|| link.clone().unwrap_or_default()),
                description: Some(desc_html),
                link,
                author: None,
                pub_date: None,
                categories: Vec::new(),
            });
        })?;

        let data = HubData {
            title: "Reuters Top News".to_string(),
            description: Some("Reuters top news stories.".to_string()),
            link: Some("https://www.reuters.com/world/".to_string()),
            image: None,
            language: None,
            items,
            allow_empty: false,
        };

        debug!(
            hub_id = ctx.hub_id,
            items = data.items.len(),
            "reuters_top hub handler"
        );

        Ok(HubResult::Data(data))
    }
}

static REUTERS_TOP_HANDLER: ReutersTopHubHandler = ReutersTopHubHandler;

/// Built-in Hub handler for medium/tag.
pub struct MediumTagHubHandler;

#[async_trait::async_trait]
impl HubHandler for MediumTagHubHandler {
    async fn handle(&self, ctx: &mut HubHandlerCtx<'_>) -> captura_common::Result<HubResult> {
        let tag = ctx.param_str("tag").unwrap_or("rust");
        let url = format!("https://medium.com/tag/{}/latest", tag);

        let mut items: Vec<HubItem> = Vec::new();
        let opts = hub_utils::HubHttpOpts::default();
        let html = hub_utils::get_html(&url, &opts, None).await?;

        hub_utils::for_each_element(&html, "div.postArticle", |el| {
            let link = crate::extract_attr(&el, "a.ds-link@href")
                .or_else(|| crate::extract_attr(&el, "a.link--primary@href"))
                .map(|href| hub_utils::absolutize(&url, &href));
            let title = crate::extract_text(&el, "h3").or_else(|| crate::extract_text(&el, "h2"));
            let desc_html = hub_utils::element_html_sanitized(&el);
            items.push(HubItem {
                title: title.unwrap_or_else(|| link.clone().unwrap_or_default()),
                description: Some(desc_html),
                link,
                author: None,
                pub_date: None,
                categories: Vec::new(),
            });
        })?;

        let data = HubData {
            title: format!("Medium Tag: {}", tag),
            description: Some("Medium posts by tag.".to_string()),
            link: Some(format!("https://medium.com/tag/{}", tag)),
            image: None,
            language: None,
            items,
            allow_empty: false,
        };

        debug!(
            hub_id = ctx.hub_id,
            items = data.items.len(),
            "medium_tag hub handler"
        );

        Ok(HubResult::Data(data))
    }
}

static MEDIUM_TAG_HANDLER: MediumTagHubHandler = MediumTagHubHandler;

fn hub_result_to_entries(res: HubResult) -> Result<Vec<NormalizedEntry>> {
    match res {
        HubResult::Data(HubData { items, .. }) => {
            let mut entries = Vec::new();
            for item in items {
                let url = item.link.clone();
                entries.push(NormalizedEntry {
                    guid: url.clone(),
                    url,
                    title: Some(item.title),
                    summary: item.description.clone(),
                    content_html: item.description,
                    author: item.author,
                    published_at: item.pub_date.map(|d| d.with_timezone(&chrono::Utc)),
                    enclosures: Vec::new(),
                    extras: serde_json::json!({}),
                });
            }
            Ok(entries)
        }
    }
}

/// Try executing a built-in Hub handler for a given rule spec (mapped by rule id).
pub(crate) async fn execute_builtin_hub_for_rule(
    feed: &feed::Model,
    spec: &RuleSpecV1,
) -> Option<Result<Vec<NormalizedEntry>>> {
    let hub_id = if let Some(rest) = spec.id.strip_prefix("captura.route.") {
        rest.replace('.', "/")
    } else {
        return None;
    };

    let params = crate::merge_rule_params_v1(spec, feed.rule_params_json.as_ref());
    let mut param_map = serde_json::Map::new();
    if let Some(val) = params {
        if let Some(obj) = val.as_object() {
            param_map = obj.clone();
        }
    }

    let mut ctx = HubHandlerCtx {
        hub_id: &hub_id,
        params: &param_map,
    };

    let handler = match find_builtin_handler(&hub_id) {
        Some(h) => h,
        None => return None,
    };

    let res = handler.handle(&mut ctx).await;
    Some(res.and_then(hub_result_to_entries))
}

/// Execute a Hub route by its id and parameters, returning `HubResult`.
pub async fn execute_hub_route(
    hub_id: &str,
    params: &serde_json::Map<String, serde_json::Value>,
) -> captura_common::Result<HubResult> {
    let handler = find_builtin_handler(hub_id).ok_or_else(|| {
        captura_common::Error::Config(format!("unknown hub route: {}", hub_id))
    })?;
    let mut ctx = HubHandlerCtx { hub_id, params };
    handler.handle(&mut ctx).await
}
