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

pub const META_SUPCHINA_PODCASTS: RouteMeta = RouteMeta {
    hub_id: "supchina/podcasts",
    path: "/supchina/podcasts",
    categories: &["new-media"],
    example: "/supchina/podcasts",
    params: &[ParamMeta {
        name: "limit",
        description: "最大节目数量（默认 50）。",
        default: Some("50"),
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["supchina.com/podcasts", "supchina.com/"],
        target: "/podcasts",
    }],
    name: "SupChina Podcasts",
    maintainers: &["captura"],
    url: "https://supchina.com/podcasts",
    description: "SupChina 播客 RSS 聚合，基于官方 https://supchina.com/feed/podcast 提供的节目列表。",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let limit = ctx.param_i64("limit").unwrap_or(50).max(1) as usize;
    let feed_url = "https://supchina.com/feed/podcast";

    let fetcher = make_fetcher()?;
    let feed = fetcher.fetch_feed(feed_url).await?;

    let feed_title = feed
        .title
        .as_ref()
        .map(|t| t.content.clone())
        .unwrap_or_else(|| "SupChina - Podcasts".to_string());
    let feed_link = feed
        .links
        .get(0)
        .map(|l| l.href.clone())
        .unwrap_or_else(|| "https://supchina.com/podcasts".to_string());
    let feed_image = feed
        .icon
        .as_ref()
        .map(|i| i.uri.clone())
        .or_else(|| feed.logo.as_ref().map(|i| i.uri.clone()));

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
        description: Some(
            "SupChina 官方播客 RSS 内容，由 captura 转换为统一 Hub 数据格式。".to_string(),
        ),
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
pub const ROUTE_SUPCHINA_PODCASTS: Route = Route {
    meta: &META_SUPCHINA_PODCASTS,
    handler: handler_fn,
};
