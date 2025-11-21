use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeZone};
use scraper::{Html, Selector};

const BASE_URL: &str = "https://rustcc.cn";
const NEWS_URL: &str = "https://rustcc.cn/section?id=f4703117-7e6b-4caf-aa22-a3ad3db6898f";

pub const META_RUSTCC_NEWS: RouteMeta = RouteMeta {
    hub_id: "rustcc/news",
    path: "/rustcc/news",
    categories: &["programming"],
    example: "/rustcc/news",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["rustcc.cn"],
        target: "/news",
    }],
    name: "Rust 语言中文社区新闻/聚合",
    maintainers: &["captura"],
    url: "https://rustcc.cn/",
    description: "Rust 语言中文社区新闻/聚合列表，对标 RSSHub /rustcc/news 路由。",
    default_view: Some("articles"),
};

fn parse_pub_date(raw: &str) -> Option<DateTime<FixedOffset>> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    if let Ok(naive) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M") {
        if let Some(offset) = FixedOffset::east_opt(8 * 3600) {
            return offset.from_local_datetime(&naive).single();
        }
    }
    None
}

fn extract_items(html: &str) -> Result<Vec<HubItem>> {
    let doc = Html::parse_document(html);
    let sel_item = Selector::parse(".article-list li")
        .map_err(|e| Error::Parse(format!("rustcc: invalid item selector: {e}")))?;
    let sel_title = Selector::parse("a.title")
        .map_err(|e| Error::Parse(format!("rustcc: invalid title selector: {e}")))?;
    let sel_tags = Selector::parse(".info .tags")
        .map_err(|e| Error::Parse(format!("rustcc: invalid tags selector: {e}")))?;
    let sel_time = Selector::parse(".info .timestamp")
        .map_err(|e| Error::Parse(format!("rustcc: invalid time selector: {e}")))?;

    let mut items = Vec::new();

    for li in doc.select(&sel_item) {
        let title_el = li.select(&sel_title).next();
        let Some(title_el) = title_el else {
            continue;
        };

        let title = title_el.text().collect::<String>().trim().to_string();
        if title.is_empty() {
            continue;
        }

        let href = title_el.value().attr("href").unwrap_or("");
        let link = if href.is_empty() {
            None
        } else {
            Some(util::absolutize(BASE_URL, href))
        };

        let desc = li
            .select(&sel_tags)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string());

        let time_raw = li
            .select(&sel_time)
            .next()
            .map(|el| el.text().collect::<String>())
            .unwrap_or_default();
        let pub_date = parse_pub_date(&time_raw);

        items.push(HubItem {
            title,
            description: desc.filter(|s| !s.is_empty()),
            link,
            author: None,
            pub_date,
            categories: Vec::new(),
        });
    }

    Ok(items)
}

pub async fn handler(_ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let html = util::get_html(NEWS_URL).await?;
    let items = extract_items(&html)?;

    Ok(HubData {
        title: "Rust 语言中文社区 | 新闻/聚合".to_string(),
        description: Some("获取 Rust 语言中文社区的新闻/聚合。".to_string()),
        link: Some(NEWS_URL.to_string()),
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
pub const ROUTE_RUSTCC_NEWS: Route = Route {
    meta: &META_RUSTCC_NEWS,
    handler: handler_fn,
};
