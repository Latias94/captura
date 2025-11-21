use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_fetcher::{FetchOptions, HttpFetcher};
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset, Utc};
use scraper::{Html, Selector};

const HOST: &str = "https://www.magnumphotos.com";

fn make_fetcher() -> captura_common::Result<HttpFetcher> {
    HttpFetcher::new(FetchOptions::default())
}

fn to_fixed_offset(dt: DateTime<Utc>) -> Option<DateTime<FixedOffset>> {
    FixedOffset::east_opt(0).map(|offset| dt.with_timezone(&offset))
}

pub const META_MAGNUMPHOTOS_MAGAZINE: RouteMeta = RouteMeta {
    hub_id: "magnumphotos/magazine",
    path: "/magnumphotos/magazine",
    categories: &["picture"],
    example: "/magnumphotos/magazine",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["magnumphotos.com"],
        target: "/magazine",
    }],
    name: "Magnum Photos Magazine",
    maintainers: &["captura"],
    url: "https://www.magnumphotos.com",
    description:
        "Magnum Photos magazine feed based on the official site RSS, aligned with RSSHub /magnumphotos/magazine route.",
    default_view: Some("pictures"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let limit = ctx.param_i64("limit").unwrap_or(20).max(1) as usize;
    let feed_url = format!("{}/feed/", HOST);

    let fetcher = make_fetcher()?;
    let feed = fetcher.fetch_feed(&feed_url).await?;

    let feed_title = feed
        .title
        .as_ref()
        .map(|t| t.content.clone())
        .unwrap_or_else(|| "Magnum Photos".to_string());
    let feed_link = feed
        .links
        .get(0)
        .map(|l| l.href.clone())
        .unwrap_or_else(|| HOST.to_string());

    let mut items = Vec::new();

    for entry in feed.entries.into_iter().take(limit) {
        let title = entry
            .title
            .as_ref()
            .map(|t| t.content.clone())
            .unwrap_or_else(|| entry.id.clone());
        let link = entry.links.get(0).map(|l| l.href.clone());
        if link.is_none() {
            continue;
        }
        let link_url = link.unwrap();

        let detail_html = util::get_html(&link_url).await.ok();
        let description = detail_html.and_then(|body| {
            let doc = Html::parse_document(&body);
            let sel = Selector::parse("#content").ok()?;

            doc.select(&sel).next().map(|el| util::element_html(&el))
        });

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
            link: Some(link_url),
            author,
            pub_date,
            categories,
        });
    }

    Ok(HubData {
        title: feed_title,
        description: Some(
            "Magnum is a community of thought and visual storytelling, via its magazine feed."
                .to_string(),
        ),
        link: Some(feed_link),
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
pub const ROUTE_MAGNUMPHOTOS_MAGAZINE: Route = Route {
    meta: &META_MAGNUMPHOTOS_MAGAZINE,
    handler: handler_fn,
};
