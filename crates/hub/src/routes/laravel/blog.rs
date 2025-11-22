use crate::routes::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use crate::routes::util;
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset};
use scraper::{Html, Selector};

const ROOT_URL: &str = "https://laravel.com";
const BLOG_URL: &str = "https://laravel.com/blog";

pub const META_LARAVEL_BLOG: RouteMeta = RouteMeta {
    hub_id: "laravel/blog",
    path: "/laravel/blog",
    categories: &["programming", "backend"],
    example: "/laravel/blog",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["laravel.com/blog"],
        target: "/blog",
    }],
    name: "Laravel 官方博客",
    maintainers: &["captura"],
    url: BLOG_URL,
    description: "Laravel 官方博客文章列表（解析 HTML，提取正文内容）。",
    default_view: Some("articles"),
};

async fn fetch_index_html() -> Result<String> {
    util::get_html(BLOG_URL).await
}

fn collect_post_links(html: &str, limit: usize) -> Result<Vec<String>> {
    let doc = Html::parse_document(html);

    let sel_article = Selector::parse("article.group")
        .map_err(|e| Error::Parse(format!("laravel/blog: invalid article selector: {e}")))?;
    let sel_title_link = Selector::parse("h2 a, h3 a")
        .map_err(|e| Error::Parse(format!("laravel/blog: invalid title link selector: {e}")))?;

    let mut links: Vec<String> = Vec::new();

    // Hero + featured cards.
    for article in doc.select(&sel_article) {
        if links.len() >= limit {
            break;
        }
        let Some(a) = article.select(&sel_title_link).next() else {
            continue;
        };
        let href = a.value().attr("href").unwrap_or("");
        if href.is_empty() {
            continue;
        }
        if href.contains("/blog/category/") {
            continue;
        }
        let link = util::absolutize(ROOT_URL, href);
        if !links.contains(&link) {
            links.push(link);
        }
    }

    // "More posts" list (text-only links), under #posts-section.
    if links.len() < limit {
        let sel_more_link = Selector::parse(r#"div#posts-section a[href]"#).map_err(|e| {
            Error::Parse(format!(
                "laravel/blog: invalid posts-section link selector: {e}"
            ))
        })?;
        for a in doc.select(&sel_more_link) {
            if links.len() >= limit {
                break;
            }
            let href = a.value().attr("href").unwrap_or("");
            if href.is_empty() {
                continue;
            }
            if !href.contains("/blog/") || href.contains("/blog/category/") {
                continue;
            }
            let link = util::absolutize(ROOT_URL, href);
            if !links.contains(&link) {
                links.push(link);
            }
        }
    }

    Ok(links)
}

fn parse_article_date(doc: &Html) -> Option<DateTime<FixedOffset>> {
    let sel_time = Selector::parse("aside time[datetime]").ok()?;
    let time_el = doc.select(&sel_time).next()?;
    let dt = time_el.value().attr("datetime")?;
    util::parse_date(dt)
}

fn parse_article_author(doc: &Html) -> Option<String> {
    // First author block in the aside.
    let sel_author = Selector::parse("aside div.mb-8 p").ok()?;
    doc.select(&sel_author)
        .next()
        .map(|p| p.text().collect::<String>().trim().to_string())
        .filter(|s| !s.is_empty())
}

fn parse_article_categories(doc: &Html) -> Vec<String> {
    let mut categories = Vec::new();
    categories.push("php".to_string());
    categories.push("laravel".to_string());

    let sel_cat = match Selector::parse(r#"aside a[href*="/blog/category/"]"#) {
        Ok(s) => s,
        Err(_) => return categories,
    };
    for a in doc.select(&sel_cat) {
        let text = a.text().collect::<String>().trim().to_string();
        if !text.is_empty() && !categories.contains(&text) {
            categories.push(text);
        }
    }

    categories
}

async fn fetch_post(url: &str) -> Result<HubItem> {
    let html = util::get_html(url).await?;
    let doc = Html::parse_document(&html);

    let sel_title = Selector::parse("header h1")
        .map_err(|e| Error::Parse(format!("laravel/blog: invalid title selector: {e}")))?;
    let sel_body = Selector::parse("article div.prose")
        .map_err(|e| Error::Parse(format!("laravel/blog: invalid body selector: {e}")))?;

    let title = doc
        .select(&sel_title)
        .next()
        .map(|h| h.text().collect::<String>().trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| url.to_string());

    let description = doc
        .select(&sel_body)
        .next()
        .map(|el| util::element_html(&el));

    let pub_date = parse_article_date(&doc);
    let author = parse_article_author(&doc);
    let categories = parse_article_categories(&doc);

    Ok(HubItem {
        title,
        description,
        link: Some(url.to_string()),
        author,
        pub_date,
        categories,
    })
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let limit = ctx.param_i64("limit").unwrap_or(20).max(1) as usize;

    let index_html = fetch_index_html().await?;
    let links = collect_post_links(&index_html, limit)?;

    let mut items = Vec::new();
    for link in links.into_iter().take(limit) {
        match fetch_post(&link).await {
            Ok(item) => items.push(item),
            Err(e) => {
                tracing::warn!("laravel/blog: failed to fetch post {}: {}", link, e);
            }
        }
    }

    Ok(HubData {
        title: "Laravel Blog".to_string(),
        description: Some("Laravel 官方博客文章列表。".to_string()),
        link: Some(BLOG_URL.to_string()),
        image: Some("https://laravel.com/images/blog/og-card-laravel-blog.png".to_string()),
        language: Some("en".to_string()),
        items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_LARAVEL_BLOG: Route = Route {
    meta: &META_LARAVEL_BLOG,
    handler: handler_fn,
};
