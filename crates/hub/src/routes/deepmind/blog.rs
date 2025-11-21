use crate::routes::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use crate::routes::util;
use captura_common::Result;
use captura_fetcher::{FetchOptions, HttpFetcher};
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset, Utc};
use scraper::{Html, Selector};

fn make_fetcher() -> Result<HttpFetcher> {
    HttpFetcher::new(FetchOptions::default())
}

fn to_fixed_offset(dt: DateTime<Utc>) -> Option<DateTime<FixedOffset>> {
    FixedOffset::east_opt(0).map(|offset| dt.with_timezone(&offset))
}

fn extract_body(html: &str) -> Option<String> {
    let doc = Html::parse_document(html);
    let sel = Selector::parse(".e_container .c_rich-text__cms").ok()?;
    let el = doc.select(&sel).next()?;
    let body = util::element_html(&el);
    if body.trim().is_empty() {
        None
    } else {
        Some(body)
    }
}

pub const META_DEEPMIND_BLOG: RouteMeta = RouteMeta {
    hub_id: "deepmind/blog",
    path: "/deepmind/blog",
    categories: &["technology"],
    example: "/deepmind/blog",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["deepmind.com/blog", "deepmind.com/"],
        target: "/blog",
    }],
    name: "DeepMind Blog",
    maintainers: &["captura"],
    url: "https://www.deepmind.com/blog",
    description:
        "Official DeepMind blog, based on the RSS feed with full article content extracted from the page.",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let limit = ctx.param_i64("limit").unwrap_or(40).max(1) as usize;
    let feed_url = "https://www.deepmind.com/blog/rss.xml";

    let fetcher = make_fetcher()?;
    let feed = match fetcher.fetch_feed(feed_url).await {
        Ok(f) => f,
        Err(e) => {
            return Ok(HubData {
                title: "DeepMind Blog".to_string(),
                description: Some(format!(
                    "DeepMind RSS feed is currently unavailable or not a valid XML feed: {}",
                    e
                )),
                link: Some("https://deepmind.google/blog/".to_string()),
                image: None,
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
        .unwrap_or_else(|| "DeepMind Blog".to_string());
    let feed_link = feed
        .links
        .get(0)
        .map(|l| l.href.clone())
        .unwrap_or_else(|| "https://www.deepmind.com/blog".to_string());
    let feed_image = feed
        .icon
        .as_ref()
        .map(|i| i.uri.clone())
        .or_else(|| feed.logo.as_ref().map(|l| l.uri.clone()));

    let mut items = Vec::new();

    for entry in feed.entries.into_iter().take(limit) {
        let title = entry
            .title
            .as_ref()
            .map(|t| t.content.clone())
            .unwrap_or_else(|| entry.id.clone());
        let link = entry.links.get(0).map(|l| l.href.clone());

        let mut description = entry
            .content
            .as_ref()
            .and_then(|c| c.body.clone())
            .or_else(|| entry.summary.as_ref().map(|s| s.content.clone()));

        if let Some(ref url) = link {
            if let Ok(html) = util::get_html(url).await {
                if let Some(full) = extract_body(&html) {
                    description = Some(full);
                }
            }
        }

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
            "DeepMind blog posts with full content extracted from article pages.".to_string(),
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
pub const ROUTE_DEEPMIND_BLOG: Route = Route {
    meta: &META_DEEPMIND_BLOG,
    handler: handler_fn,
};
