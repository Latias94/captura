use crate::routes::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset};
use scraper::{Html, Selector};

const ROOT_URL: &str = "https://thegradient.pub";

fn parse_pub_date(s: &str) -> Option<DateTime<FixedOffset>> {
    util::parse_date(s)
}

fn extract_list(
    html: &str,
    limit: usize,
) -> captura_common::Result<Vec<(String, String, Option<DateTime<FixedOffset>>)>> {
    let doc = Html::parse_document(html);
    let sel_wrap = Selector::parse(".c-post-card-wrap")
        .map_err(|e| Error::Parse(format!("thegradient: card selector error: {e}")))?;
    let sel_link = Selector::parse(".c-post-card__title-link")
        .map_err(|e| Error::Parse(format!("thegradient: link selector error: {e}")))?;
    let sel_meta = Selector::parse(".c-post-card__meta time")
        .map_err(|e| Error::Parse(format!("thegradient: meta selector error: {e}")))?;

    let mut out = Vec::new();

    for wrap in doc.select(&sel_wrap).take(limit) {
        let link_el = wrap.select(&sel_link).next();
        let href = link_el
            .and_then(|el| el.value().attr("href"))
            .map(|h| util::absolutize(ROOT_URL, h));
        let title = link_el
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();
        if href.is_none() || title.is_empty() {
            continue;
        }

        let date_str = wrap
            .select(&sel_meta)
            .next()
            .and_then(|el| el.value().attr("datetime"))
            .unwrap_or("")
            .to_string();
        let pub_date = if date_str.is_empty() {
            None
        } else {
            parse_pub_date(&date_str)
        };

        out.push((title, href.unwrap(), pub_date));
    }

    Ok(out)
}

fn extract_article_body(html: &str) -> Option<String> {
    let doc = Html::parse_document(html);
    let sel = Selector::parse(".c-content").ok()?;
    let el = doc.select(&sel).next()?;
    let body = el.html();
    if body.trim().is_empty() {
        None
    } else {
        Some(body)
    }
}

pub const META_THEGRADIENT_POSTS: RouteMeta = RouteMeta {
    hub_id: "thegradient/posts",
    path: "/thegradient/posts",
    categories: &["technology"],
    example: "/thegradient/posts",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["thegradient.pub/"],
        target: "/posts",
    }],
    name: "The Gradient Posts",
    maintainers: &["captura"],
    url: "https://thegradient.pub/",
    description: "The Gradient blog posts with full article content, aligned with RSSHub /thegradient/posts route.",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let limit = ctx.param_i64("limit").unwrap_or(40).max(1) as usize;
    let url = ROOT_URL.to_string();
    let html = util::get_html(&url).await?;

    let list = extract_list(&html, limit)?;

    let mut items = Vec::new();

    for (title, link, pub_date) in list {
        let mut description = None;
        if let Ok(article_html) = util::get_html(&link).await {
            description = extract_article_body(&article_html);
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
        title: "The Gradient Blog".to_string(),
        description: Some(
            "Essays and posts from The Gradient, focusing on machine learning and AI.".to_string(),
        ),
        link: Some(url),
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
pub const ROUTE_THEGRADIENT_POSTS: Route = Route {
    meta: &META_THEGRADIENT_POSTS,
    handler: handler_fn,
};
