use crate::routes::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use captura_hub_macros::register_hub_route;

use captura_fetcher::{FetchOptions, HttpFetcher};

const RSS_URL: &str = "https://www.pcgamer.com/rss/";
const SITE_URL: &str = "https://www.pcgamer.com";

pub const META_PCGAMER_LATEST: RouteMeta = RouteMeta {
    hub_id: "pcgamer/latest",
    path: "/pcgamer/latest",
    categories: &["game"],
    example: "/pcgamer/latest",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["www.pcgamer.com", "www.pcgamer.com/rss/*"],
        target: "/latest",
    }],
    name: "PC Gamer - Latest",
    maintainers: &["captura"],
    url: SITE_URL,
    description: "Latest articles from PC Gamer via the official RSS feed.",
    default_view: Some("articles"),
};

pub async fn handler(_ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let fetcher = HttpFetcher::new(FetchOptions::default())
        .map_err(|e| captura_common::Error::Network(e.to_string()))?;
    let feed = fetcher.fetch_feed(RSS_URL).await?;

    let title = feed
        .title
        .as_ref()
        .map(|t| t.content.clone())
        .unwrap_or_else(|| "PC Gamer latest".to_string());
    let description = feed.description.as_ref().map(|d| d.content.clone());

    let link = feed
        .links
        .iter()
        .find(|l| l.rel == Some("alternate".to_string()))
        .or_else(|| feed.links.first())
        .map(|l| l.href.clone())
        .unwrap_or_else(|| SITE_URL.to_string());

    let mut items = Vec::new();
    for entry in feed.entries {
        let title = match entry.title.as_ref() {
            Some(t) => t.content.trim().to_string(),
            None => continue,
        };
        if title.is_empty() {
            continue;
        }

        let link = entry
            .links
            .iter()
            .find(|l| l.rel == Some("alternate".to_string()))
            .or_else(|| entry.links.first())
            .map(|l| l.href.clone());

        let pub_date = entry.published;

        let description = if let Some(content) = entry.content.as_ref() {
            content.body.clone()
        } else if let Some(summary) = entry.summary.as_ref() {
            Some(summary.content.clone())
        } else {
            None
        };

        let mut categories = Vec::new();
        for c in &entry.categories {
            if let Some(label) = &c.label {
                if !label.is_empty() {
                    categories.push(label.clone());
                }
            }
        }
        if categories.is_empty() {
            categories.push("pcgamer".to_string());
        }

        items.push(HubItem {
            title,
            description,
            link,
            author: entry.authors.get(0).and_then(|a| {
                let name = a.name.clone();
                if name.is_empty() { None } else { Some(name) }
            }),
            pub_date: pub_date
                .and_then(|dt| chrono::FixedOffset::east_opt(0).map(|off| dt.with_timezone(&off))),
            categories,
        });
    }

    Ok(HubData {
        title,
        description,
        link: Some(link),
        image: None,
        language: feed.language.clone(),
        items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_PCGAMER_LATEST: Route = Route {
    meta: &META_PCGAMER_LATEST,
    handler: handler_fn,
};
