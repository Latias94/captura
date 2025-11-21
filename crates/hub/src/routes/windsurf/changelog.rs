use crate::routes::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use crate::routes::util;
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset};
use scraper::{Html, Selector};

const BASE_URL: &str = "https://windsurf.com";

pub const META_WINDSURF_CHANGELOG: RouteMeta = RouteMeta {
    hub_id: "windsurf/changelog",
    path: "/windsurf/changelog",
    categories: &["programming"],
    example: "/windsurf/changelog",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["windsurf.com/changelog"],
        target: "/changelog",
    }],
    name: "Windsurf Changelog",
    maintainers: &["captura"],
    url: "https://windsurf.com/changelog",
    description: "Windsurf 发布日志，对标 RSSHub /windsurf/changelog 路由。",
    default_view: Some("articles"),
};

fn parse_pub_date(raw: &str) -> Option<DateTime<FixedOffset>> {
    util::parse_date(raw)
}

fn extract_items(html: &str, limit: usize) -> Result<Vec<HubItem>> {
    let doc = Html::parse_document(html);
    let sel_block = Selector::parse("div[aria-label=\"changelog-layout\"]")
        .map_err(|e| Error::Parse(format!("windsurf: invalid changelog selector: {e}")))?;

    let sel_header_div = Selector::parse("header div")
        .map_err(|e| Error::Parse(format!("windsurf: invalid header selector: {e}")))?;
    let sel_h1 = Selector::parse("article h1")
        .map_err(|e| Error::Parse(format!("windsurf: invalid h1 selector: {e}")))?;
    let sel_article_div = Selector::parse("article div")
        .map_err(|e| Error::Parse(format!("windsurf: invalid article div selector: {e}")))?;
    let sel_img = Selector::parse("article img")
        .map_err(|e| Error::Parse(format!("windsurf: invalid img selector: {e}")))?;

    let mut items = Vec::new();

    for (idx, block) in doc.select(&sel_block).enumerate() {
        if idx >= limit {
            break;
        }

        let version = block
            .select(&sel_header_div)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string());
        let heading = block
            .select(&sel_h1)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string());
        let title = match (version, heading) {
            (Some(v), Some(h)) => format!("{} {}", v, h),
            (Some(v), None) => v,
            (None, Some(h)) => h,
            _ => "Windsurf 更新".to_string(),
        };

        let desc = block
            .select(&sel_article_div)
            .next()
            .map(|el| util::element_html(&el));
        let date_raw = block
            .select(&sel_header_div)
            .last()
            .map(|el| el.text().collect::<String>().trim().to_string());
        let pub_date = date_raw.as_deref().and_then(parse_pub_date);

        let image = block
            .select(&sel_img)
            .next()
            .and_then(|el| el.value().attr("src"))
            .map(|s| s.to_string());

        let mut html_desc = desc.unwrap_or_default();
        if let Some(img) = image {
            if !img.is_empty() {
                let img_html = format!(
                    "<p><img src=\"{src}\" alt=\"{alt}\"></p>",
                    src = img,
                    alt = title
                );
                if html_desc.is_empty() {
                    html_desc = img_html;
                } else {
                    html_desc = format!("{}{}", img_html, html_desc);
                }
            }
        }

        items.push(HubItem {
            title,
            description: if html_desc.is_empty() {
                None
            } else {
                Some(html_desc)
            },
            link: Some(format!("{}/changelog", BASE_URL)),
            author: None,
            pub_date,
            categories: Vec::new(),
        });
    }

    Ok(items)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let limit = ctx.param_i64("limit").unwrap_or(100).max(1) as usize;
    let url = format!("{}/changelog", BASE_URL);
    let html = util::get_html(&url).await?;
    let items = extract_items(&html, limit)?;

    Ok(HubData {
        title: "Windsurf Changelog".to_string(),
        description: Some("Windsurf 发布日志与版本更新。".to_string()),
        link: Some(url),
        image: None,
        language: None,
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_WINDSURF_CHANGELOG: Route = Route {
    meta: &META_WINDSURF_CHANGELOG,
    handler: handler_fn,
};
