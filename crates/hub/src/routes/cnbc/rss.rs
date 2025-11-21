use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::Error;
use captura_fetcher::{FetchOptions, HttpFetcher};
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset, Utc};
use scraper::{Html, Selector};
use serde_json::Value;

fn make_fetcher() -> captura_common::Result<HttpFetcher> {
    HttpFetcher::new(FetchOptions::default())
}

fn to_fixed_offset(dt: DateTime<Utc>) -> Option<DateTime<FixedOffset>> {
    FixedOffset::east_opt(0).map(|offset| dt.with_timezone(&offset))
}

fn extract_article(html: &str) -> (Option<String>, Option<String>, Vec<String>) {
    let doc = Html::parse_document(html);

    let selectors = [
        ".FeaturedContent-articleBody",
        ".ArticleBody-articleBody",
        ".LiveBlogBody-articleBody",
        ".ClipPlayer-clipPlayer",
    ];

    let mut body_html = String::new();
    for sel_str in selectors.iter() {
        if let Ok(sel) = Selector::parse(sel_str) {
            if let Some(el) = doc.select(&sel).next() {
                body_html.push_str(&el.html());
            }
        }
    }
    let description = if body_html.trim().is_empty() {
        None
    } else {
        Some(body_html)
    };

    let mut author: Option<String> = None;
    let mut categories: Vec<String> = Vec::new();

    if let Ok(sel) = Selector::parse(r#"script[type="application/ld+json"]"#) {
        let mut last_json = None;
        for el in doc.select(&sel) {
            let text = el.text().collect::<String>();
            if !text.trim().is_empty() {
                last_json = Some(text);
            }
        }
        if let Some(json_str) = last_json {
            if let Ok(mut v) = serde_json::from_str::<Value>(&json_str) {
                if let Some(arr) = v.as_array() {
                    if let Some(found) = arr
                        .iter()
                        .rev()
                        .find(|obj| obj.get("author").is_some() || obj.get("headline").is_some())
                    {
                        v = found.clone();
                    }
                }
                if let Some(a) = v.get("author") {
                    match a {
                        Value::String(s) => {
                            if !s.trim().is_empty() {
                                author = Some(s.trim().to_string());
                            }
                        }
                        Value::Object(obj) => {
                            if let Some(Value::String(name)) = obj.get("name") {
                                if !name.trim().is_empty() {
                                    author = Some(name.trim().to_string());
                                }
                            }
                        }
                        Value::Array(arr) => {
                            let mut names = Vec::new();
                            for item in arr {
                                if let Some(obj) = item.as_object() {
                                    if let Some(Value::String(name)) = obj.get("name") {
                                        if !name.trim().is_empty() {
                                            names.push(name.trim().to_string());
                                        }
                                    }
                                } else if let Some(s) = item.as_str() {
                                    if !s.trim().is_empty() {
                                        names.push(s.trim().to_string());
                                    }
                                }
                            }
                            if !names.is_empty() {
                                author = Some(names.join(", "));
                            }
                        }
                        _ => {}
                    }
                }

                if let Some(kw) = v.get("keywords") {
                    match kw {
                        Value::String(s) => {
                            if !s.trim().is_empty() {
                                categories.push(s.trim().to_string());
                            }
                        }
                        Value::Array(arr) => {
                            for item in arr {
                                if let Some(s) = item.as_str() {
                                    if !s.trim().is_empty() {
                                        categories.push(s.trim().to_string());
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    (description, author, categories)
}

pub const META_CNBC_RSS: RouteMeta = RouteMeta {
    hub_id: "cnbc/rss",
    path: "/cnbc/rss/:id?",
    categories: &["traditional-media"],
    example: "/cnbc/rss",
    params: &[ParamMeta {
        name: "id",
        description:
            "Channel ID from official CNBC RSS URLs, defaults to 100003114 (Top News).",
        default: Some("100003114"),
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["cnbc.com/id/:id/device/rss/rss.html"],
        target: "/rss/:id",
    }],
    name: "CNBC full article RSS",
    maintainers: &["captura"],
    url: "https://www.cnbc.com/rss-feeds/",
    description:
        "Full-article CNBC RSS feeds based on the combinedcms endpoint, aligned with RSSHub /cnbc/rss.",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let id = ctx.param_str("id").unwrap_or("100003114");
    let limit = ctx.param_i64("limit").unwrap_or(40).max(1) as usize;
    let feed_url = format!(
        "https://search.cnbc.com/rs/search/combinedcms/view.xml?partnerId=wrss01&id={}",
        id
    );

    let fetcher = make_fetcher()?;
    let feed = fetcher.fetch_feed(&feed_url).await?;

    let feed_title = feed
        .title
        .as_ref()
        .map(|t| t.content.clone())
        .unwrap_or_else(|| "CNBC".to_string());
    let feed_link = feed
        .links
        .get(0)
        .map(|l| l.href.clone())
        .unwrap_or_else(|| "https://www.cnbc.com/".to_string());

    let mut items = Vec::new();

    for entry in feed.entries.into_iter().take(limit) {
        let title = entry
            .title
            .as_ref()
            .map(|t| t.content.clone())
            .unwrap_or_else(|| entry.id.clone());

        let link = entry.links.get(0).map(|l| l.href.clone());
        if let Some(ref url) = link {
            if url.starts_with("https://www.cnbc.com/select/") {
                continue;
            }
        }

        let mut description = entry
            .content
            .as_ref()
            .and_then(|c| c.body.clone())
            .or_else(|| entry.summary.as_ref().map(|s| s.content.clone()));

        let mut author = if entry.authors.is_empty() {
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

        let mut categories = entry
            .categories
            .iter()
            .map(|c| c.term.clone())
            .collect::<Vec<_>>();

        if let Some(ref url) = link {
            if let Ok(html) = util::get_html(url).await {
                let (body, author_ld, cats_ld) = extract_article(&html);
                if let Some(full) = body {
                    description = Some(full);
                }
                if let Some(a) = author_ld {
                    author = Some(a);
                }
                if !cats_ld.is_empty() {
                    categories = cats_ld;
                }
            }
        }

        let pub_date = entry.published.or(entry.updated).and_then(to_fixed_offset);

        items.push(HubItem {
            title,
            description,
            link,
            author,
            pub_date,
            categories,
        });
    }

    Ok(HubData {
        title: feed_title,
        description: Some("CNBC full-article RSS feed.".to_string()),
        link: Some(feed_link),
        image: None,
        language: feed.language.clone(),
        items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_CNBC_RSS: Route = Route {
    meta: &META_CNBC_RSS,
    handler: handler_fn,
};
