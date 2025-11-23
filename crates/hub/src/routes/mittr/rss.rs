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

pub const META_MITTR_RSS: RouteMeta = RouteMeta {
    hub_id: "mittr/rss",
    path: "/mittr/rss",
    categories: &["technology"],
    example: "/mittr/rss",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["www.technologyreview.com"],
        target: "/rss",
    }],
    name: "MIT Technology Review",
    maintainers: &["captura"],
    url: "https://www.technologyreview.com",
    description: "Official RSS feed from MIT Technology Review (global edition) with full article metadata.",
    default_view: Some("articles"),
};

pub async fn handler(_ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let feed_url = "https://www.technologyreview.com/feed/";

    let fetcher = make_fetcher()?;
    let feed = fetcher.fetch_feed(feed_url).await?;

    let feed_title = feed
        .title
        .as_ref()
        .map(|t| t.content.clone())
        .unwrap_or_else(|| "MIT Technology Review".to_string());
    let feed_link = feed
        .links
        .get(0)
        .map(|l| l.href.clone())
        .unwrap_or_else(|| "https://www.technologyreview.com".to_string());
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
        description: Some("MIT Technology Review global RSS feed.".to_string()),
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
pub const ROUTE_MITTR_RSS: Route = Route {
    meta: &META_MITTR_RSS,
    handler: handler_fn,
};
