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

pub const META_OPENAI_NEWS: RouteMeta = RouteMeta {
    hub_id: "openai/news",
    path: "/openai/news",
    categories: &["technology"],
    example: "/openai/news",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["openai.com/news"],
        target: "/news",
    }],
    name: "OpenAI News",
    maintainers: &["captura"],
    url: "https://openai.com/news",
    description: "Official OpenAI News RSS feed, covering product updates, partnerships, and policy news.",
    default_view: Some("articles"),
};

pub async fn handler(_ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let feed_url = "https://openai.com/news/rss.xml";

    let fetcher = make_fetcher()?;
    let feed = fetcher.fetch_feed(feed_url).await?;

    let feed_title = feed
        .title
        .as_ref()
        .map(|t| t.content.clone())
        .unwrap_or_else(|| "OpenAI News".to_string());
    let feed_link = feed
        .links
        .get(0)
        .map(|l| l.href.clone())
        .unwrap_or_else(|| "https://openai.com/news".to_string());

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
        description: Some("Official OpenAI News updates (based on RSS entries).".to_string()),
        link: Some(feed_link),
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
pub const ROUTE_OPENAI_NEWS: Route = Route {
    meta: &META_OPENAI_NEWS,
    handler: handler_fn,
};
