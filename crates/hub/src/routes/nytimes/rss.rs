use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
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

pub const META_NYTIMES_RSS: RouteMeta = RouteMeta {
    hub_id: "nytimes/rss",
    path: "/nytimes/rss/:cat",
    categories: &["traditional-media"],
    example: "/nytimes/rss/HomePage",
    params: &[ParamMeta {
        name: "cat",
        description: "NYTimes RSS category name, matching the last segment in https://www.nytimes.com/rss",
        default: Some("HomePage"),
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["www.nytimes.com"],
        target: "/rss",
    }],
    name: "NYTimes RSS",
    maintainers: &["captura"],
    url: "https://www.nytimes.com/rss",
    description: "New York Times English RSS feeds backed by official RSS endpoints.",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let cat = ctx.param_str("cat").unwrap_or("HomePage");
    let feed_url = format!("https://rss.nytimes.com/services/xml/rss/nyt/{}.xml", cat);

    let fetcher = make_fetcher()?;
    let feed = fetcher.fetch_feed(&feed_url).await?;

    let feed_title = feed
        .title
        .as_ref()
        .map(|t| t.content.clone())
        .unwrap_or_else(|| format!("NYTimes - {}", cat));
    let feed_link = feed
        .links
        .get(0)
        .map(|l| l.href.clone())
        .unwrap_or_else(|| "https://www.nytimes.com/".to_string());
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

        // Start from feed-provided summary/content as a fallback.
        let mut description = entry
            .content
            .as_ref()
            .and_then(|c| c.body.clone())
            .or_else(|| entry.summary.as_ref().map(|s| s.content.clone()));

        // RSSHub-style enhancement: fetch article page and extract the full body
        // from the `name="articleBody"` container when possible, and optional
        // author from `meta[name="byl"]`.
        let mut author = if entry.authors.is_empty() {
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

        if let Some(link_url) = &link {
            if let Ok(html) = util::get_html(link_url).await {
                // Parse article HTML once.
                let doc = scraper::Html::parse_document(&html);

                // Try to extract full article body.
                if let Ok(sel) = scraper::Selector::parse("[name='articleBody']") {
                    if let Some(el) = doc.select(&sel).next() {
                        let body_html = util::element_html(&el);
                        if !body_html.trim().is_empty() {
                            description = Some(body_html);
                        }
                    }
                }

                // Try to override author with byline meta if present.
                if let Ok(sel) = scraper::Selector::parse("meta[name='byl']") {
                    if let Some(el) = doc.select(&sel).next() {
                        if let Some(byl) = el.value().attr("content") {
                            if !byl.trim().is_empty() {
                                author = Some(byl.trim().to_string());
                            }
                        }
                    }
                }
            }
        }

        let pub_date = entry.published.or(entry.updated).and_then(to_fixed_offset);
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
        description: Some(format!("NYTimes RSS - {}", cat)),
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
pub const ROUTE_NYTIMES_RSS: Route = Route {
    meta: &META_NYTIMES_RSS,
    handler: handler_fn,
};
