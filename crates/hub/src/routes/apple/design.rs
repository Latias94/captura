use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;
use chrono::{DateTime, FixedOffset};
use scraper::{Html, Selector};

pub const META_APPLE_DESIGN: RouteMeta = RouteMeta {
    hub_id: "apple/design",
    path: "/apple/design",
    categories: &["design"],
    example: "/apple/design",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["developer.apple.com/design/whats-new"],
        target: "/design",
    }],
    name: "Apple Design updates",
    maintainers: &["captura"],
    url: "https://developer.apple.com/design/whats-new/",
    description: "Official Apple Design updates, aligned with RSSHub /apple/design route.",
    default_view: Some("articles"),
};

fn parse_date(raw: &str) -> Option<DateTime<FixedOffset>> {
    crate::routes::util::parse_date(raw)
}

async fn fetch_html(url: &str) -> Result<String> {
    let client = client_basic(None, None).map_err(|e| Error::Network(e.to_string()))?;
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!("{} -> http status {}", url, status)));
    }
    resp.text().await.map_err(|e| Error::Network(e.to_string()))
}

fn extract_items(html: &str) -> Result<Vec<HubItem>> {
    let doc = Html::parse_document(html);
    let sel_table = Selector::parse("table")
        .map_err(|e| Error::Parse(format!("apple design: invalid table selector: {e}")))?;
    let sel_date = Selector::parse(".date")
        .map_err(|e| Error::Parse(format!("apple design: invalid date selector: {e}")))?;
    let sel_topic = Selector::parse(".topic-item")
        .map_err(|e| Error::Parse(format!("apple design: invalid topic selector: {e}")))?;
    let sel_title = Selector::parse("span.topic-title a")
        .map_err(|e| Error::Parse(format!("apple design: invalid title selector: {e}")))?;
    let sel_desc = Selector::parse("span.description")
        .map_err(|e| Error::Parse(format!("apple design: invalid desc selector: {e}")))?;

    let mut items = Vec::new();

    for table in doc.select(&sel_table) {
        let date_text = table
            .select(&sel_date)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();
        let pub_date = parse_date(&date_text);

        for row in table.select(&sel_topic) {
            let title_el = row.select(&sel_title).next();
            let Some(title_el) = title_el else {
                continue;
            };
            let title = title_el.text().collect::<String>().trim().to_string();
            if title.is_empty() {
                continue;
            }
            let href = title_el.value().attr("href").unwrap_or("");
            if href.is_empty() {
                continue;
            }
            let link = crate::routes::util::absolutize("https://developer.apple.com", href);

            let desc = row
                .select(&sel_desc)
                .next()
                .map(|el| el.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            items.push(HubItem {
                title,
                description: if desc.is_empty() { None } else { Some(desc) },
                link: Some(link),
                author: None,
                pub_date,
                categories: Vec::new(),
            });
        }
    }

    Ok(items)
}

pub async fn handler(_ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let url = "https://developer.apple.com/design/whats-new/";
    let html = fetch_html(url).await?;
    let items = extract_items(&html)?;

    Ok(HubData {
        title: "Apple design updates".to_string(),
        description: Some("Official updates from Apple Design.".to_string()),
        link: Some(url.to_string()),
        image: None,
        language: Some("en".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_APPLE_DESIGN: Route = Route {
    meta: &META_APPLE_DESIGN,
    handler: handler_fn,
};
