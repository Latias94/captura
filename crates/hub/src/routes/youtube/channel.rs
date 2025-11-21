use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::Result;
use captura_fetcher::{FetchOptions, HttpFetcher};
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset, Utc};

const FEED_BASE: &str = "https://www.youtube.com/feeds/videos.xml";

fn make_fetcher() -> Result<HttpFetcher> {
    HttpFetcher::new(FetchOptions::default())
}

fn to_fixed_offset(dt: DateTime<Utc>) -> Option<DateTime<FixedOffset>> {
    FixedOffset::east_opt(0).map(|offset| dt.with_timezone(&offset))
}

pub const META_YOUTUBE_CHANNEL: RouteMeta = RouteMeta {
    hub_id: "youtube/channel",
    path: "/youtube/channel/:channel_id",
    categories: &["multimedia"],
    example: "/youtube/channel/UCDwDMPOZfxVV0x_dz0eQ8KQ",
    params: &[ParamMeta {
        name: "channel_id",
        description: "YouTube channel id (starts with UC...).",
        default: None,
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["www.youtube.com/channel/:channel_id"],
        target: "/channel/:channel_id",
    }],
    name: "YouTube Channel (RSS)",
    maintainers: &["captura"],
    url: "https://www.youtube.com",
    description:
        "YouTube channel videos via the official RSS feed, aligned with RSSHub /youtube/channel route in spirit but implemented using feeds.",
    default_view: Some("videos"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let channel_id = ctx.param_str("channel_id").unwrap_or("").trim().to_string();
    if channel_id.is_empty() {
        return Err(captura_common::Error::Parse(
            "channel_id is required".to_string(),
        ));
    }

    let limit = ctx.param_i64("limit").unwrap_or(20).max(1) as usize;
    let feed_url = format!("{}?channel_id={}", FEED_BASE, channel_id);

    let fetcher = make_fetcher()?;
    let feed = fetcher.fetch_feed(&feed_url).await?;

    let title = feed
        .title
        .as_ref()
        .map(|t| t.content.clone())
        .unwrap_or_else(|| format!("YouTube Channel {}", channel_id));
    let link = feed
        .links
        .get(0)
        .map(|l| l.href.clone())
        .unwrap_or_else(|| format!("https://www.youtube.com/channel/{}", channel_id));
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
pub const ROUTE_YOUTUBE_CHANNEL: Route = Route {
    meta: &META_YOUTUBE_CHANNEL,
    handler: handler_fn,
};
