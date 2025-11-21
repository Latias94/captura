use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;
use chrono::{DateTime, FixedOffset};
use regex::Regex;
use scraper::{Html, Selector};

const BASE_URL: &str = "https://www.python.org";

pub const META_PYTHON_RELEASE: RouteMeta = RouteMeta {
    hub_id: "python/release",
    path: "/python/release",
    categories: &["programming"],
    example: "/python/release",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["www.python.org", "www.python.org/downloads"],
        target: "/release",
    }],
    name: "Active Python Releases",
    maintainers: &["captura"],
    url: "https://www.python.org/downloads/",
    description: "Python.org Active Python Releases，对标 RSSHub /python/release 路由。",
    default_view: Some("articles"),
};

fn parse_date(s: &str) -> Option<DateTime<FixedOffset>> {
    util::parse_date(s)
}

async fn fetch_downloads_page() -> Result<String> {
    util::get_html(&format!("{}/downloads", BASE_URL)).await
}

fn extract_list(html: &str, limit: usize) -> Result<Vec<(String, Option<String>, Option<String>)>> {
    let doc = Html::parse_document(html);
    let sel_li = Selector::parse("div.active-release-list-widget ol.list-row-container li")
        .map_err(|e| Error::Parse(format!("python: invalid list selector: {e}")))?;
    let sel_version = Selector::parse("span.release-version")
        .map_err(|e| Error::Parse(format!("python: invalid version selector: {e}")))?;
    let sel_start = Selector::parse("span.release-start")
        .map_err(|e| Error::Parse(format!("python: invalid start selector: {e}")))?;
    let sel_pep = Selector::parse("span.release-pep a")
        .map_err(|e| Error::Parse(format!("python: invalid pep selector: {e}")))?;

    let re = Regex::new(r"(\d{4}-\d{2}-\d{2})").unwrap();

    let mut out = Vec::new();
    for li in doc.select(&sel_li).take(limit) {
        let title = li
            .select(&sel_version)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();
        if title.is_empty() {
            continue;
        }

        let date_text = li
            .select(&sel_start)
            .next()
            .map(|el| el.text().collect::<String>())
            .unwrap_or_default();
        let date = re
            .captures(&date_text)
            .and_then(|cap| cap.get(1).map(|m| m.as_str().to_string()));

        let link = li
            .select(&sel_pep)
            .next()
            .and_then(|el| el.value().attr("href"))
            .map(|s| s.to_string());

        out.push((title, date, link));
    }

    Ok(out)
}

async fn fetch_detail(
    link: &str,
    fallback_title: &str,
) -> Result<(String, Option<String>, Option<String>)> {
    let html = util::get_html(link).await?;
    let doc = Html::parse_document(&html);
    let sel_title = Selector::parse("h1.page-title")
        .map_err(|e| Error::Parse(format!("python: invalid title selector: {e}")))?;
    let sel_content = Selector::parse("section#pep-content")
        .map_err(|e| Error::Parse(format!("python: invalid pep-content selector: {e}")))?;
    let sel_img = Selector::parse("meta[property=\"og:image\"]")
        .map_err(|e| Error::Parse(format!("python: invalid og:image selector: {e}")))?;

    let title = doc
        .select(&sel_title)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| fallback_title.to_string());

    let description = doc.select(&sel_content).next().map(|el| el.html());

    let image = doc
        .select(&sel_img)
        .next()
        .and_then(|el| el.value().attr("content"))
        .map(|s| s.to_string());

    Ok((title, description, image))
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let limit = ctx.param_i64("limit").unwrap_or(30).max(1) as usize;
    let html = fetch_downloads_page().await?;
    let list = extract_list(&html, limit)?;

    let mut items = Vec::new();
    for (version, date_str, link_opt) in list {
        let mut title = version.clone();
        let mut description = None;
        let mut image = None;

        if let Some(link) = &link_opt {
            if let Ok((t, desc, img)) = fetch_detail(link, &version).await {
                title = t;
                description = desc;
                image = img;
            }
        }

        let pub_date = date_str.as_deref().and_then(parse_date);

        let mut categories = Vec::new();
        categories.push("Python".to_string());

        items.push(HubItem {
            title,
            description,
            link: link_opt.clone(),
            author: None,
            pub_date,
            categories,
        });
    }

    let doc = Html::parse_document(&html);
    let sel_widget_title = Selector::parse("div.active-release-list-widget h2.widget-title")
        .map_err(|e| Error::Parse(format!("python: invalid widget-title selector: {e}")))?;
    let sel_meta_desc = Selector::parse("meta[property=\"og:description\"]")
        .map_err(|e| Error::Parse(format!("python: invalid og:description selector: {e}")))?;
    let sel_meta_img = Selector::parse("meta[property=\"og:image\"]")
        .map_err(|e| Error::Parse(format!("python: invalid og:image selector: {e}")))?;

    let title = doc
        .select(&sel_widget_title)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .unwrap_or_else(|| "Active Python Releases".to_string());
    let description = doc
        .select(&sel_meta_desc)
        .next()
        .and_then(|el| el.value().attr("content"))
        .map(|s| s.to_string());
    let image = doc
        .select(&sel_meta_img)
        .next()
        .and_then(|el| el.value().attr("content"))
        .map(|s| s.to_string());

    Ok(HubData {
        title,
        description,
        link: Some(format!("{}/downloads", BASE_URL)),
        image,
        language: Some("en".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_PYTHON_RELEASE: Route = Route {
    meta: &META_PYTHON_RELEASE,
    handler: handler_fn,
};
