use crate::routes::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use captura_common::Result;
use captura_fetcher::{FetchOptions, HttpFetcher};
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset, Utc};

fn make_fetcher() -> Result<HttpFetcher> {
    HttpFetcher::new(FetchOptions::default())
}

fn to_fixed_offset(dt: DateTime<Utc>) -> Option<DateTime<FixedOffset>> {
    FixedOffset::east_opt(0).map(|offset| dt.with_timezone(&offset))
}

pub const META_VUE_BLOG: RouteMeta = RouteMeta {
    hub_id: "vue/blog",
    path: "/vue/blog",
    categories: &["programming", "frontend"],
    example: "/vue/blog",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["blog.vuejs.org"],
        target: "/blog",
    }],
    name: "Vue 官方博客",
    maintainers: &["captura"],
    url: "https://blog.vuejs.org",
    description: "The Vue Point 官方博客（基于 RSS feed，包含内容摘要）。",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let limit = ctx.param_i64("limit").unwrap_or(30).max(1) as usize;
    let feed_url = "https://blog.vuejs.org/feed.rss";

    let fetcher = make_fetcher()?;
    let feed = match fetcher.fetch_feed(feed_url).await {
        Ok(f) => f,
        Err(e) => {
            return Ok(HubData {
                title: "The Vue Point".to_string(),
                description: Some(format!(
                    "Vue RSS feed is currently unavailable or not a valid XML feed: {}",
                    e
                )),
                link: Some("https://blog.vuejs.org".to_string()),
                image: Some("https://vuejs.org/images/logo.png".to_string()),
                language: Some("en".to_string()),
                items: Vec::new(),
                allow_empty: true,
            });
        }
    };

    let feed_title = feed
        .title
        .as_ref()
        .map(|t| t.content.clone())
        .unwrap_or_else(|| "The Vue Point".to_string());
    let feed_link = feed
        .links
        .get(0)
        .map(|l| l.href.clone())
        .unwrap_or_else(|| "https://blog.vuejs.org".to_string());
    let feed_image = feed
        .icon
        .as_ref()
        .map(|i| i.uri.clone())
        .or_else(|| feed.logo.as_ref().map(|l| l.uri.clone()))
        .or_else(|| Some("https://vuejs.org/images/logo.png".to_string()));

    let mut items = Vec::new();

    for entry in feed.entries.into_iter().take(limit) {
        let title = entry
            .title
            .as_ref()
            .map(|t| t.content.clone())
            .unwrap_or_else(|| entry.id.clone());
        let link = entry.links.get(0).map(|l| l.href.clone());

        let description = entry
            .content
            .as_ref()
            .and_then(|c| c.body.clone())
            .or_else(|| entry.summary.as_ref().map(|s| s.content.clone()));

        let pub_date = entry.published.or(entry.updated).and_then(to_fixed_offset);
        let author = if entry.authors.is_empty() {
            None
        } else {
            Some(
                entry
                    .authors
                    .iter()
                    .map(|p| p.name.clone())
                    .collect::<Vec<_>>()
                    .join(", "),
            )
        };
        let mut categories = entry
            .categories
            .iter()
            .map(|c| c.term.clone())
            .collect::<Vec<_>>();
        if !categories.contains(&"vue".to_string()) {
            categories.push("vue".to_string());
        }

        items.push(HubItem {
            title,
            description,
            link,
            author,
            pub_date,
            categories,
        });
    }

    Ok(HubData {
        title: feed_title,
        description: Some("Vue 官方博客（The Vue Point），基于 RSS feed。".to_string()),
        link: Some(feed_link),
        image: feed_image,
        language: feed.language.clone().or_else(|| Some("en".to_string())),
        items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_VUE_BLOG: Route = Route {
    meta: &META_VUE_BLOG,
    handler: handler_fn,
};
