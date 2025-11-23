use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset};
use scraper::{Html, Selector};
use serde::Deserialize;
use serde_json::Value;

const ROOT_URL: &str = "https://www.theatlantic.com";

#[derive(Debug, Deserialize)]
struct RiverEdge {
    node: RiverNode,
}

#[derive(Debug, Deserialize)]
struct RiverNode {
    url: String,
    #[serde(rename = "datePublished")]
    date_published: Option<String>,
}

fn parse_pub_date(s: &str) -> Option<DateTime<FixedOffset>> {
    util::parse_date(s)
}

fn extract_urql_state(html: &str) -> captura_common::Result<Value> {
    let doc = Html::parse_document(html);
    let sel = Selector::parse(r#"script#__NEXT_DATA__"#)
        .map_err(|e| Error::Parse(format!("theatlantic: next_data selector error: {e}")))?;
    let script = doc
        .select(&sel)
        .next()
        .ok_or_else(|| Error::Parse("theatlantic: __NEXT_DATA__ not found".to_string()))?;
    let json_str = script.text().collect::<String>();
    serde_json::from_str(&json_str)
        .map_err(|e| Error::Parse(format!("theatlantic: __NEXT_DATA__ parse error: {e}")))
}

fn extract_river_edges(state: &Value, category: &str) -> captura_common::Result<Vec<RiverEdge>> {
    let urql = state
        .get("props")
        .and_then(|v| v.get("pageProps"))
        .and_then(|v| v.get("urqlState"))
        .ok_or_else(|| Error::Parse("theatlantic: missing urqlState".to_string()))?;

    let mut target_value: Option<Value> = None;
    if let Some(obj) = urql.as_object() {
        for (key, val) in obj {
            if let Some(data_str) = val.get("data").and_then(|d| d.as_str()) {
                if data_str.contains(category) {
                    target_value = Some(Value::String(data_str.to_string()));
                    break;
                }
            }
        }
    }
    let data_str = target_value
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .ok_or_else(|| {
            Error::Parse("theatlantic: river data not found for category".to_string())
        })?;

    let data_json: Value = serde_json::from_str(&data_str)
        .map_err(|e| Error::Parse(format!("theatlantic: river json parse error: {e}")))?;

    let first_val = data_json
        .as_object()
        .and_then(|obj| obj.values().next())
        .ok_or_else(|| Error::Parse("theatlantic: river edges missing".to_string()))?;
    let edges = first_val
        .get("river")
        .and_then(|v| v.get("edges"))
        .and_then(|v| v.as_array())
        .ok_or_else(|| Error::Parse("theatlantic: river.edges is not array".to_string()))?;

    let mut out = Vec::new();
    for edge in edges {
        let e: RiverEdge = serde_json::from_value(edge.clone())
            .map_err(|err| Error::Parse(format!("theatlantic: river edge decode error: {err}")))?;
        out.push(e);
    }
    Ok(out)
}

fn extract_article_from_state(
    state: &Value,
) -> captura_common::Result<(String, String, Vec<String>, Option<String>)> {
    let urql = state
        .get("props")
        .and_then(|v| v.get("pageProps"))
        .and_then(|v| v.get("urqlState"))
        .ok_or_else(|| Error::Parse("theatlantic: article urqlState missing".to_string()))?;

    let mut target: Option<String> = None;
    if let Some(obj) = urql.as_object() {
        for (_k, v) in obj {
            if let Some(data_str) = v.get("data").and_then(|d| d.as_str()) {
                if data_str.contains("content") {
                    target = Some(data_str.to_string());
                }
            }
        }
    }
    let data_str =
        target.ok_or_else(|| Error::Parse("theatlantic: article data not found".to_string()))?;

    let article_root: Value = serde_json::from_str(&data_str)
        .map_err(|e| Error::Parse(format!("theatlantic: article json parse error: {e}")))?;
    let article = article_root
        .get("article")
        .ok_or_else(|| Error::Parse("theatlantic: article field missing".to_string()))?;

    let title = article
        .get("shareTitle")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();

    let mut categories = Vec::new();
    if let Some(cats) = article.get("categories").and_then(|v| v.as_array()) {
        for c in cats {
            if let Some(slug) = c.get("slug").and_then(|v| v.as_str()) {
                if !slug.is_empty() {
                    categories.push(slug.to_string());
                }
            }
        }
    }
    if let Some(channels) = article.get("channels").and_then(|v| v.as_array()) {
        for ch in channels {
            if let Some(slug) = ch.get("slug").and_then(|v| v.as_str()) {
                if !slug.is_empty() {
                    categories.push(slug.to_string());
                }
            }
        }
    }

    let caption = article
        .get("dek")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string());

    let mut body_html = String::new();
    if let Some(content) = article.get("content").and_then(|v| v.as_array()) {
        for block in content {
            if let Some(inner) = block.get("innerHtml").and_then(|v| v.as_str()) {
                if !inner.trim().is_empty() {
                    body_html.push_str(inner);
                }
            }
        }
    }

    let mut description = String::new();
    if let Some(c) = &caption {
        if !c.is_empty() {
            description.push_str("<p><em>");
            description.push_str(c);
            description.push_str("</em></p>");
        }
    }
    description.push_str(&body_html);

    Ok((title, description, categories, caption))
}

pub const META_THEATLANTIC_NEWS: RouteMeta = RouteMeta {
    hub_id: "theatlantic/news",
    path: "/theatlantic/:category",
    categories: &["traditional-media"],
    example: "/theatlantic/latest",
    params: &[ParamMeta {
        name: "category",
        description: "Section slug, such as most-popular, latest, politics, technology, business, etc.",
        default: Some("latest"),
        options: &[
            ("most-popular", "Popular"),
            ("latest", "Latest"),
            ("politics", "Politics"),
            ("technology", "Technology"),
            ("business", "Business"),
        ],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["theatlantic.com/:category"],
        target: "/:category",
    }],
    name: "The Atlantic News",
    maintainers: &["captura"],
    url: "https://www.theatlantic.com",
    description: "The Atlantic news river for a given section (popular, latest, politics, technology, business, ...), aligned with RSSHub /theatlantic/:category route.",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let category = ctx.param_str("category").unwrap_or("latest");
    let limit = ctx.param_i64("limit").unwrap_or(30).max(1) as usize;

    let url = format!("{ROOT_URL}/{category}/");
    let html = util::get_html(&url).await?;

    let next_state = extract_urql_state(&html)?;
    let edges = extract_river_edges(&next_state, category)?;

    let mut items = Vec::new();

    for edge in edges.into_iter().take(limit) {
        if edge
            .node
            .url
            .starts_with("https://www.theatlantic.com/photo")
        {
            continue;
        }
        let link = edge.node.url.clone();
        let pub_date = edge.node.date_published.as_deref().and_then(parse_pub_date);

        let article_html = util::get_html(&link).await?;
        let article_state = extract_urql_state(&article_html)?;
        let (title, description, categories, _caption) =
            extract_article_from_state(&article_state)?;

        if title.is_empty() {
            continue;
        }

        items.push(HubItem {
            title,
            description: Some(description),
            link: Some(link),
            author: None,
            pub_date,
            categories,
        });
    }

    let feed_title = format!("The Atlantic - {}", category.to_uppercase());

    Ok(HubData {
        title: feed_title.clone(),
        description: Some(feed_title),
        link: Some(url),
        image: None,
        language: Some("en-US".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_THEATLANTIC_NEWS: Route = Route {
    meta: &META_THEATLANTIC_NEWS,
    handler: handler_fn,
};
