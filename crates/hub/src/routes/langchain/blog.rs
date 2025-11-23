use crate::routes::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset};
use scraper::{Html, Selector};

const ROOT_URL: &str = "https://blog.langchain.dev";

fn parse_pub_date(_s: &str) -> Option<DateTime<FixedOffset>> {
    // LangChain list page does not expose a canonical date; individual posts
    // may contain date metadata, which can be added later if needed.
    None
}

fn extract_post_list(
    html: &str,
    limit: usize,
) -> captura_common::Result<Vec<(String, String, Option<String>)>> {
    let doc = Html::parse_document(html);
    let sel_card = Selector::parse(".posts-feed .post-card")
        .map_err(|e| Error::Parse(format!("langchain: card selector error: {e}")))?;
    let sel_link = Selector::parse(".post-card__content-link")
        .map_err(|e| Error::Parse(format!("langchain: link selector error: {e}")))?;
    let sel_title = Selector::parse(".post-card__title")
        .map_err(|e| Error::Parse(format!("langchain: title selector error: {e}")))?;
    let sel_excerpt = Selector::parse(".post-card__excerpt")
        .map_err(|e| Error::Parse(format!("langchain: excerpt selector error: {e}")))?;

    let mut items = Vec::new();

    for card in doc.select(&sel_card).take(limit) {
        let link_el = card.select(&sel_link).next();
        let href = link_el
            .and_then(|el| el.value().attr("href"))
            .map(|h| util::absolutize(ROOT_URL, h));

        let title = card
            .select(&sel_title)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();

        if href.is_none() || title.is_empty() {
            continue;
        }

        let excerpt = card
            .select(&sel_excerpt)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty());

        items.push((title, href.unwrap(), excerpt));
    }

    Ok(items)
}

fn extract_post_body(html: &str) -> Option<String> {
    let doc = Html::parse_document(html);
    let sel = Selector::parse(".article-content").ok()?;
    let el = doc.select(&sel).next()?;
    let body = util::element_html(&el);
    if body.trim().is_empty() {
        None
    } else {
        Some(body)
    }
}

pub const META_LANGCHAIN_BLOG: RouteMeta = RouteMeta {
    hub_id: "langchain/blog",
    path: "/langchain/blog",
    categories: &["technology"],
    example: "/langchain/blog",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["blog.langchain.dev/"],
        target: "/blog",
    }],
    name: "LangChain Blog",
    maintainers: &["captura"],
    url: "https://blog.langchain.dev/",
    description: "LangChain official blog posts, with full article content extracted from each post page.",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let limit = ctx.param_i64("limit").unwrap_or(40).max(1) as usize;
    let html = util::get_html(ROOT_URL).await?;

    let list = extract_post_list(&html, limit)?;

    let mut items = Vec::new();

    for (title, link, excerpt) in list.into_iter() {
        let mut description = excerpt.clone();

        if let Ok(post_html) = util::get_html(&link).await {
            if let Some(full) = extract_post_body(&post_html) {
                description = Some(full);
            }
        }

        items.push(HubItem {
            title,
            description,
            link: Some(link),
            author: None,
            pub_date: None,
            categories: Vec::new(),
        });
    }

    Ok(HubData {
        title: "LangChain Blog".to_string(),
        description: Some("Articles and updates from the LangChain official blog.".to_string()),
        link: Some(ROOT_URL.to_string()),
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
pub const ROUTE_LANGCHAIN_BLOG: Route = Route {
    meta: &META_LANGCHAIN_BLOG,
    handler: handler_fn,
};
