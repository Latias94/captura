use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::Result;
use captura_fetcher::{FetchOptions, HttpFetcher};
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset, Utc};
use scraper::Html;
use serde_json::Value;

fn make_fetcher() -> Result<HttpFetcher> {
    HttpFetcher::new(FetchOptions::default())
}

fn to_fixed_offset(dt: DateTime<Utc>) -> Option<DateTime<FixedOffset>> {
    FixedOffset::east_opt(0).map(|offset| dt.with_timezone(&offset))
}

pub const META_THEVERGE_RSS: RouteMeta = RouteMeta {
    hub_id: "theverge/rss",
    path: "/theverge/:hub?",
    categories: &["new-media"],
    example: "/theverge",
    params: &[ParamMeta {
        name: "hub",
        description: "The Verge hub slug, e.g. 'apple', 'android', 'gaming'; empty for All Posts",
        default: None,
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["www.theverge.com"],
        target: "/rss",
    }],
    name: "The Verge RSS",
    maintainers: &["captura"],
    url: "https://www.theverge.com/",
    description: "The Verge category feeds backed by official RSS endpoints, with optional Next.js full-text parsing.",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let hub = ctx.param_str("hub").unwrap_or("");
    let feed_url = if hub.is_empty() {
        "https://www.theverge.com/rss/index.xml".to_string()
    } else {
        format!("https://www.theverge.com/rss/{}/index.xml", hub)
    };

    let fetcher = make_fetcher()?;
    let feed = fetcher.fetch_feed(&feed_url).await?;

    let feed_title = feed
        .title
        .as_ref()
        .map(|t| t.content.clone())
        .unwrap_or_else(|| {
            if hub.is_empty() {
                "The Verge".to_string()
            } else {
                format!("The Verge - {}", hub)
            }
        });
    let feed_link = feed
        .links
        .get(0)
        .map(|l| l.href.clone())
        .unwrap_or_else(|| "https://www.theverge.com/".to_string());
    let feed_image = feed
        .icon
        .as_ref()
        .map(|i| i.uri.clone())
        .or_else(|| feed.logo.as_ref().map(|i| i.uri.clone()));

    let mut items = Vec::new();

    for entry in feed.entries {
        let title = entry
            .title
            .as_ref()
            .map(|t| t.content.clone())
            .unwrap_or_else(|| entry.id.clone());
        let link = entry.links.get(0).map(|l| l.href.clone());
        let mut description = entry
            .content
            .as_ref()
            .and_then(|c| c.body.clone())
            .or_else(|| entry.summary.as_ref().map(|s| s.content.clone()));
        let pub_date = entry.published.or(entry.updated).and_then(to_fixed_offset);
        let author = if entry.authors.is_empty() {
            None
        } else {
            Some(
                entry
                    .authors
                    .iter()
                    .map(|p| p.name.clone())
                    .collect::<Vec<_>>()
                    .join(", "),
            )
        };
        let categories = entry
            .categories
            .iter()
            .map(|c| c.term.clone())
            .collect::<Vec<_>>();

        // Optional Next.js enhancement: try to replace description with the
        // full article body extracted from __NEXT_DATA__. On failure, fall
        // back to the feed-provided HTML.
        if let Some(link_url) = &link {
            if let Ok(html) = util::get_html(link_url).await {
                if let Some(full_html) = extract_next_body(&html) {
                    description = Some(full_html);
                }
            }
        }

        items.push(HubItem {
            title,
            description,
            link,
            author,
            pub_date,
            categories,
        });
    }

    let desc = if hub.is_empty() {
        "The Verge - All Posts".to_string()
    } else {
        format!("The Verge hub: {}", hub)
    };

    Ok(HubData {
        title: feed_title,
        description: Some(desc),
        link: Some(feed_link),
        image: feed_image,
        language: feed.language.clone(),
        items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_THEVERGE_RSS: Route = Route {
    meta: &META_THEVERGE_RSS,
    handler: handler_fn,
};

fn extract_next_body(html: &str) -> Option<String> {
    let doc = Html::parse_document(html);
    let sel = scraper::Selector::parse("script#__NEXT_DATA__").ok()?;
    let script = doc.select(&sel).next()?;
    let text = script.text().collect::<String>();
    if text.trim().is_empty() {
        return None;
    }
    let value: Value = serde_json::from_str(text.trim()).ok()?;

    // Navigate to hydration responses and find the relevant node.
    let responses = value
        .pointer("/props/pageProps/hydration/responses")
        .and_then(|v| v.as_array())?;

    let mut node_opt: Option<&Value> = None;
    for r in responses {
        let op = r
            .get("operationName")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if op == "PostLayoutQuery" || op == "StreamLayoutQuery" || node_opt.is_none() {
            if let Some(node) = r.get("data").and_then(|d| d.get("node")) {
                node_opt = Some(node);
                if op == "PostLayoutQuery" {
                    break;
                }
            }
        }
    }
    let node = node_opt?;
    let blocks = node.get("blocks").and_then(|b| b.as_array())?;

    let mut out = String::new();
    for b in blocks {
        let chunk = render_block(b);
        if !chunk.trim().is_empty() {
            if !out.is_empty() {
                out.push_str("<br><br>");
            }
            out.push_str(&chunk);
        }
    }

    if out.trim().is_empty() {
        None
    } else {
        Some(out)
    }
}

fn render_block(b: &Value) -> String {
    let t = b
        .get("__typename")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    match t {
        "CoreHeadingBlockType" => {
            let level = b
                .get("level")
                .and_then(|v| v.as_u64())
                .unwrap_or(2)
                .clamp(1, 6);
            let content = b
                .get("contents")
                .and_then(|c| c.get("html"))
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            format!("<h{lvl}>{}</h{lvl}>", content, lvl = level)
        }
        "CoreParagraphBlockType" => {
            let mut buf = String::new();
            if let Some(arr) = b.get("tempContents").and_then(|v| v.as_array()) {
                for c in arr {
                    if let Some(html) = c.get("html").and_then(|v| v.as_str()) {
                        buf.push_str(html);
                    }
                }
            }
            buf
        }
        "CoreHTMLBlockType" => b
            .get("markup")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        "CoreImageBlockType" => {
            let url = b
                .get("thumbnail")
                .and_then(|t| t.get("url"))
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let alt = b.get("alt").and_then(|v| v.as_str()).unwrap_or_default();
            let caption = b
                .get("caption")
                .and_then(|c| c.get("html"))
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if url.is_empty() {
                String::new()
            } else {
                format!(
                    "<figure><img src=\"{src}\" alt=\"{alt}\"><figcaption>{cap}</figcaption></figure>",
                    src = strip_query(url),
                    alt = alt,
                    cap = caption
                )
            }
        }
        "CoreListBlockType" => {
            let ordered = b.get("ordered").and_then(|v| v.as_bool()).unwrap_or(false);
            let tag_open = if ordered { "<ol>" } else { "<ul>" };
            let tag_close = if ordered { "</ol>" } else { "</ul>" };
            let mut buf = String::new();
            buf.push_str(tag_open);
            if let Some(items) = b.get("items").and_then(|v| v.as_array()) {
                for it in items {
                    let html = it
                        .get("contents")
                        .and_then(|c| c.get("html"))
                        .and_then(|v| v.as_str())
                        .unwrap_or_default();
                    buf.push_str("<li>");
                    buf.push_str(html);
                    buf.push_str("</li>");
                }
            }
            buf.push_str(tag_close);
            buf
        }
        "CorePullquoteBlockType" => b
            .get("contents")
            .and_then(|c| c.get("html"))
            .and_then(|v| v.as_str())
            .map(|s| format!("<blockquote>{}</blockquote>", s))
            .unwrap_or_default(),
        "CoreQuoteBlockType" => {
            let mut inner = String::new();
            if let Some(children) = b.get("children").and_then(|v| v.as_array()) {
                for child in children {
                    inner.push_str(&render_block(child));
                }
            }
            if inner.is_empty() {
                String::new()
            } else {
                format!("<blockquote>{}</blockquote>", inner)
            }
        }
        "CoreSeparatorBlockType" => "<hr>".to_string(),
        "HighlightBlockType" => {
            let mut inner = String::new();
            if let Some(children) = b.get("children").and_then(|v| v.as_array()) {
                for child in children {
                    inner.push_str(&render_block(child));
                }
            }
            inner
        }
        _ => String::new(),
    }
}

fn strip_query(url: &str) -> String {
    match url.split_once('?') {
        Some((base, _)) => base.to_string(),
        None => url.to_string(),
    }
}
