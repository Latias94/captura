use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::Result;
use captura_fetcher::{FetchOptions, HttpFetcher};
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset, Utc};

const BASE_URL: &str = "https://tailwindcss.com";

fn make_fetcher() -> Result<HttpFetcher> {
    HttpFetcher::new(FetchOptions::default())
}

fn to_fixed_offset(dt: DateTime<Utc>) -> Option<DateTime<FixedOffset>> {
    FixedOffset::east_opt(0).map(|offset| dt.with_timezone(&offset))
}

pub const META_TAILWINDCSS_BLOG: RouteMeta = RouteMeta {
    hub_id: "tailwindcss/blog",
    path: "/tailwindcss/blog",
    categories: &["programming"],
    example: "/tailwindcss/blog",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["tailwindcss.com/blog"],
        target: "/blog",
    }],
    name: "Tailwind CSS Blog",
    maintainers: &["captura"],
    url: "https://tailwindcss.com/blog",
    description: "Official Tailwind CSS blog, aligned with RSSHub /tailwindcss/blog (via the official Atom feed).",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let limit = ctx.param_i64("limit").unwrap_or(10).max(1) as usize;
    let feed_url = format!("{}/feeds/atom.xml", BASE_URL);

    let fetcher = make_fetcher()?;
    let feed = fetcher.fetch_feed(&feed_url).await?;

    let title = feed
        .title
        .as_ref()
        .map(|t| t.content.clone())
        .unwrap_or_else(|| "Tailwind CSS Blog".to_string());
    let link = feed
        .links
        .get(0)
        .map(|l| l.href.clone())
        .unwrap_or_else(|| format!("{}/blog", BASE_URL));
    let description = feed.description.as_ref().map(|d| d.content.clone());
    let image = feed
        .icon
        .as_ref()
        .map(|i| i.uri.clone())
        .or_else(|| feed.logo.as_ref().map(|i| i.uri.clone()));

    let mut items = Vec::new();

    for entry in feed.entries.iter().take(limit) {
        let title = entry
            .title
            .as_ref()
            .map(|t| t.content.clone())
            .unwrap_or_else(|| entry.id.clone());
        let link = entry.links.get(0).map(|l| l.href.clone());
        let description_entry = entry
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
            description: description_entry,
            link,
            author,
            pub_date,
            categories,
        });
    }

    Ok(HubData {
        title,
        description,
        link: Some(link),
        image,
        language: feed.language.clone(),
        items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_TAILWINDCSS_BLOG: Route = Route {
    meta: &META_TAILWINDCSS_BLOG,
    handler: handler_fn,
};
