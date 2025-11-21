use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset};
use scraper::{Html, Selector};

pub const META_QWENLM_BLOG: RouteMeta = RouteMeta {
    hub_id: "qwenlm/blog",
    path: "/qwenlm/blog/:lang?",
    categories: &["blog"],
    example: "/qwenlm/blog/zh",
    params: &[ParamMeta {
        name: "lang",
        description: "博客语言，例如 zh、en。默认无前缀（英文）。",
        default: Some(""),
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["qwenlm.github.io/blog/", "qwenlm.github.io/:lang/blog/"],
        target: "/blog/:lang?",
    }],
    name: "Qwen Blog",
    maintainers: &["captura"],
    url: "https://qwenlm.github.io/blog",
    description: "Qwen 官方博客，对标 RSSHub /qwenlm/blog/:lang 路由。",
    default_view: Some("articles"),
};

fn build_url(lang: &str) -> String {
    if lang.is_empty() {
        "https://qwenlm.github.io/blog".to_string()
    } else {
        format!("https://qwenlm.github.io/{}/blog", lang)
    }
}

fn parse_date(raw: &str) -> Option<DateTime<FixedOffset>> {
    util::parse_date(raw)
}

fn extract_list(html: &str, limit: usize) -> Result<Vec<HubItem>> {
    let doc = Html::parse_document(html);
    let sel_article = Selector::parse("article.post-entry")
        .map_err(|e| Error::Parse(format!("qwenlm: invalid post-entry selector: {e}")))?;
    let sel_footer_span = Selector::parse(".entry-footer span")
        .map_err(|e| Error::Parse(format!("qwenlm: invalid footer span selector: {e}")))?;
    let sel_header = Selector::parse("header.entry-header h2")
        .map_err(|e| Error::Parse(format!("qwenlm: invalid header selector: {e}")))?;
    let sel_entry_link = Selector::parse(".entry-link")
        .map_err(|e| Error::Parse(format!("qwenlm: invalid entry-link selector: {e}")))?;

    let mut items = Vec::new();

    for article in doc.select(&sel_article).take(limit) {
        let date_raw = article
            .select(&sel_footer_span)
            .next()
            .and_then(|el| el.value().attr("title"))
            .unwrap_or("")
            .trim()
            .trim_end_matches("+0800")
            .to_string();

        let pub_date = parse_date(&date_raw);

        let title = article
            .select(&sel_header)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();
        if title.is_empty() {
            continue;
        }

        let link = article
            .select(&sel_entry_link)
            .next()
            .and_then(|el| el.value().attr("href"))
            .map(|s| s.to_string());

        items.push(HubItem {
            title,
            description: None,
            link,
            author: None,
            pub_date,
            categories: Vec::new(),
        });
    }

    Ok(items)
}

async fn enrich_item(mut item: HubItem) -> Result<HubItem> {
    if let Some(link) = &item.link {
        if let Ok(html) = util::get_html(link).await {
            let doc = Html::parse_document(&html);
            let sel_main = Selector::parse("main")
                .map_err(|e| Error::Parse(format!("qwenlm: invalid main selector: {e}")))?;
            if let Some(main) = doc.select(&sel_main).next() {
                let html = util::element_html(&main);
                if !html.trim().is_empty() {
                    item.description = Some(html);
                }
            }
        }
    }
    Ok(item)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let lang = ctx.param_str("lang").unwrap_or("");
    let limit = ctx.param_i64("limit").unwrap_or(30).max(1) as usize;
    let url = build_url(lang);

    let html = util::get_html(&url).await?;
    let list = extract_list(&html, limit)?;

    let mut items = Vec::new();
    for item in list {
        match enrich_item(item).await {
            Ok(i) => items.push(i),
            Err(_) => {}
        }
    }

    Ok(HubData {
        title: "Qwen Blog".to_string(),
        description: Some("Qwen 官方博客文章。".to_string()),
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
pub const ROUTE_QWENLM_BLOG: Route = Route {
    meta: &META_QWENLM_BLOG,
    handler: handler_fn,
};
