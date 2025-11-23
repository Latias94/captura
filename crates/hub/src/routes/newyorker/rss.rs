use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
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

pub const META_NEWYORKER_RSS: RouteMeta = RouteMeta {
    hub_id: "newyorker/rss",
    path: "/newyorker/rss/:section?",
    categories: &["traditional-media"],
    example: "/newyorker/rss/news",
    params: &[ParamMeta {
        name: "section",
        description: "New Yorker section name used in feed URLs, e.g. news, culture, books-and-fiction.",
        default: Some("news"),
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["www.newyorker.com"],
        target: "/rss/:section?",
    }],
    name: "The New Yorker RSS",
    maintainers: &["captura"],
    url: "https://www.newyorker.com",
    description: "Official RSS feeds from The New Yorker, parameterized by section (news, culture, etc.).",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let section = ctx.param_str("section").unwrap_or("news");
    let section = if section.is_empty() { "news" } else { section };
    let feed_url = format!("https://www.newyorker.com/feed/{}", section);

    let fetcher = make_fetcher()?;
    let feed = fetcher.fetch_feed(&feed_url).await?;

    let feed_title = feed
        .title
        .as_ref()
        .map(|t| t.content.clone())
        .unwrap_or_else(|| format!("The New Yorker - {}", section));
    let feed_link = feed
        .links
        .get(0)
        .map(|l| l.href.clone())
        .unwrap_or_else(|| "https://www.newyorker.com".to_string());
    let feed_image = feed
        .icon
        .as_ref()
        .map(|i| i.uri.clone())
        .or_else(|| feed.logo.as_ref().map(|i| i.uri.clone()));

    let mut items = Vec::new();

    for entry in feed.entries {
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
        let categories = entry
            .categories
            .iter()
            .map(|c| c.term.clone())
            .collect::<Vec<_>>();

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
        description: Some(format!("The New Yorker RSS section: {}", section)),
        link: Some(feed_link),
        image: feed_image,
        language: feed.language.clone(),
        items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_NEWYORKER_RSS: Route = Route {
    meta: &META_NEWYORKER_RSS,
    handler: handler_fn,
};
