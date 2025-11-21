use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::Result;
use captura_fetcher::{FetchOptions, HttpFetcher};
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset, Utc};
use scraper::{Html, Selector};

const ROOT_URL: &str = "https://nextjs.org";

fn make_fetcher() -> Result<HttpFetcher> {
    HttpFetcher::new(FetchOptions::default())
}

fn to_fixed_offset(dt: DateTime<Utc>) -> Option<DateTime<FixedOffset>> {
    FixedOffset::east_opt(0).map(|offset| dt.with_timezone(&offset))
}

pub const META_NEXTJS_BLOG: RouteMeta = RouteMeta {
    hub_id: "nextjs/blog",
    path: "/nextjs/blog",
    categories: &["program-update"],
    example: "/nextjs/blog",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["nextjs.org/blog"],
        target: "/blog",
    }],
    name: "Next.js Blog",
    maintainers: &["captura"],
    url: "https://nextjs.org/blog",
    description: "Official Next.js blog, in a simplified form aligned with RSSHub /nextjs/blog.",
    default_view: Some("articles"),
};

async fn fetch_blog_index() -> Result<String> {
    util::get_html(&format!("{}/blog", ROOT_URL)).await
}

fn extract_links(html: &str, limit: usize) -> Vec<String> {
    let doc = Html::parse_document(html);
    let sel_article = match Selector::parse("article a[href^=\"/blog\"]") {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    for a in doc.select(&sel_article).take(limit) {
        if let Some(href) = a.value().attr("href") {
            out.push(format!("{}{}", ROOT_URL, href));
        }
    }
    out
}

async fn fetch_post(url: &str) -> Result<HubItem> {
    let html = util::get_html(url).await?;
    let doc = Html::parse_document(&html);

    let sel_title = Selector::parse("h1").unwrap();
    let title = doc
        .select(&sel_title)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .unwrap_or_else(|| url.to_string());

    let sel_body = Selector::parse("div.prose").unwrap();
    let description = doc
        .select(&sel_body)
        .next()
        .map(|el| util::element_html(&el));

    // Next.js 博客页面中日期通常在 meta 或正文前几行，这里不强行解析，留空。
    Ok(HubItem {
        title,
        description,
        link: Some(url.to_string()),
        author: None,
        pub_date: None,
        categories: Vec::new(),
    })
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let limit = ctx.param_i64("limit").unwrap_or(20).max(1) as usize;
    let html = fetch_blog_index().await?;
    let links = extract_links(&html, limit);

    let mut items = Vec::new();
    for link in links {
        match fetch_post(&link).await {
            Ok(item) => items.push(item),
            Err(_) => {}
        }
    }

    Ok(HubData {
        title: "Next.js Blog".to_string(),
        description: Some("Official articles from the Next.js blog.".to_string()),
        link: Some(format!("{}/blog", ROOT_URL)),
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
pub const ROUTE_NEXTJS_BLOG: Route = Route {
    meta: &META_NEXTJS_BLOG,
    handler: handler_fn,
};
