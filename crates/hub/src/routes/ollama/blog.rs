use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset, NaiveDate, TimeZone};
use scraper::{Html, Selector};

const BASE_URL: &str = "https://ollama.com";

pub const META_OLLAMA_BLOG: RouteMeta = RouteMeta {
    hub_id: "ollama/blog",
    path: "/ollama/blog",
    categories: &["programming"],
    example: "/ollama/blog",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["ollama.com/blog"],
        target: "/blog",
    }],
    name: "Ollama Blog",
    maintainers: &["captura"],
    url: "https://ollama.com/blog",
    description: "Official Ollama blog post list, aligned with RSSHub /ollama/blog route.",
    default_view: Some("articles"),
};

fn parse_pub_date(raw: &str) -> Option<DateTime<FixedOffset>> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    // e.g. "November 19, 2025"
    if let Ok(date) = NaiveDate::parse_from_str(s, "%B %e, %Y")
        .or_else(|_| NaiveDate::parse_from_str(s, "%B %d, %Y"))
    {
        if let Some(naive) = date.and_hms_opt(0, 0, 0) {
            if let Some(offset) = FixedOffset::east_opt(0) {
                return Some(offset.from_utc_datetime(&naive));
            }
        }
    }
    None
}

fn extract_items(html: &str) -> Result<Vec<HubItem>> {
    let doc = Html::parse_document(html);
    let sel_item = Selector::parse("a.group.border-b.py-10")
        .map_err(|e| Error::Parse(format!("ollama: invalid item selector: {e}")))?;
    let sel_title = Selector::parse("h2")
        .map_err(|e| Error::Parse(format!("ollama: invalid title selector: {e}")))?;
    let sel_date = Selector::parse("h3")
        .map_err(|e| Error::Parse(format!("ollama: invalid date selector: {e}")))?;
    let sel_desc = Selector::parse("p")
        .map_err(|e| Error::Parse(format!("ollama: invalid desc selector: {e}")))?;

    let mut items = Vec::new();
    for a in doc.select(&sel_item) {
        let title = a
            .select(&sel_title)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();
        if title.is_empty() {
            continue;
        }

        let href = a.value().attr("href").unwrap_or("");
        if href.is_empty() {
            continue;
        }
        let link = util::absolutize(BASE_URL, href);

        let date_raw = a
            .select(&sel_date)
            .next()
            .map(|el| el.text().collect::<String>())
            .unwrap_or_default();
        let pub_date = parse_pub_date(&date_raw);

        let desc = a
            .select(&sel_desc)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string());

        items.push(HubItem {
            title,
            description: desc.filter(|s| !s.is_empty()),
            link: Some(link),
            author: None,
            pub_date,
            categories: Vec::new(),
        });
    }

    Ok(items)
}

pub async fn handler(_ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let url = format!("{}/blog", BASE_URL);
    let html = util::get_html(&url).await?;
    let items = extract_items(&html)?;

    Ok(HubData {
        title: "Ollama Blog".to_string(),
        description: Some("Official posts from the Ollama blog.".to_string()),
        link: Some(url),
        image: None,
        language: None,
        items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_OLLAMA_BLOG: Route = Route {
    meta: &META_OLLAMA_BLOG,
    handler: handler_fn,
};
