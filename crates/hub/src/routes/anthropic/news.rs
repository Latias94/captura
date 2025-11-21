use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset};
use scraper::{Html, Selector};

const ROOT_URL: &str = "https://www.anthropic.com";

pub const META_ANTHROPIC_NEWS: RouteMeta = RouteMeta {
    hub_id: "anthropic/news",
    path: "/anthropic/news",
    categories: &["programming"],
    example: "/anthropic/news",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["www.anthropic.com/news", "www.anthropic.com"],
        target: "/news",
    }],
    name: "Anthropic News",
    maintainers: &["captura"],
    url: "https://www.anthropic.com/news",
    description: "Official Anthropic news, aligned with RSSHub /anthropic/news route.",
    default_view: Some("articles"),
};

fn parse_pub_date(raw: &str) -> Option<DateTime<FixedOffset>> {
    util::parse_date(raw)
}

fn extract_list(
    html: &str,
    limit: usize,
) -> Result<Vec<(String, String, Option<DateTime<FixedOffset>>)>> {
    let doc = Html::parse_document(html);
    let sel_a = Selector::parse(".contentFadeUp a")
        .map_err(|e| Error::Parse(format!("anthropic: invalid list selector: {e}")))?;
    let sel_title = Selector::parse("h3")
        .map_err(|e| Error::Parse(format!("anthropic: invalid title selector: {e}")))?;
    let sel_date = Selector::parse("p.detail-m.agate, div[class^=\"PostList_post-date__\"]")
        .map_err(|e| Error::Parse(format!("anthropic: invalid date selector: {e}")))?;

    let mut out = Vec::new();

    for a in doc.select(&sel_a).take(limit) {
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
        let link = util::absolutize(ROOT_URL, href);

        let date_text = a
            .select(&sel_date)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();
        let pub_date = parse_pub_date(&date_text);

        out.push((title, link, pub_date));
    }

    Ok(out)
}

fn extract_article(html: &str) -> Option<String> {
    let doc = Html::parse_document(html);
    let sel_main = Selector::parse("#main-content").ok()?;
    let content = doc.select(&sel_main).next()?;

    let html = util::element_html(&content);
    if html.trim().is_empty() {
        None
    } else {
        Some(html)
    }
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let limit = ctx.param_i64("limit").unwrap_or(20).max(1) as usize;
    let url = format!("{}/news", ROOT_URL);
    let html = util::get_html(&url).await?;
    let list = extract_list(&html, limit)?;

    let mut items = Vec::new();

    for (title, link, pub_date) in list {
        let mut description = None;
        if let Ok(article_html) = util::get_html(&link).await {
            description = extract_article(&article_html);
        }

        items.push(HubItem {
            title,
            description,
            link: Some(link),
            author: None,
            pub_date,
            categories: Vec::new(),
        });
    }

    Ok(HubData {
        title: "Anthropic News".to_string(),
        description: Some("Latest news from Anthropic".to_string()),
        link: Some(url),
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
pub const ROUTE_ANTHROPIC_NEWS: Route = Route {
    meta: &META_ANTHROPIC_NEWS,
    handler: handler_fn,
};
