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

pub const META_BBC_NEWS: RouteMeta = RouteMeta {
    hub_id: "bbc/news",
    path: "/bbc/:site?/:channel?",
    categories: &["traditional-media"],
    example: "/bbc/world-asia",
    params: &[
        ParamMeta {
            name: "site",
            description:
                "language or channel slug; e.g. 'world-asia', 'chinese', 'traditionalchinese'",
            default: None,
            options: &[],
        },
        ParamMeta {
            name: "channel",
            description:
                "sub-channel for Chinese sites; e.g. 'china', 'world'; empty for top stories",
            default: None,
            options: &[],
        },
    ],
    features: Features::basic(),
    radar: &[Radar {
        source: &["www.bbc.co.uk", "www.bbc.com"],
        target: "/news",
    }],
    name: "BBC News",
    maintainers: &["captura"],
    url: "https://www.bbc.co.uk/news",
    description:
        "BBC News via official RSS feeds, including English and Chinese top stories (simplified).",
    default_view: Some("articles"),
};

fn build_bbc_feed(site: Option<&str>, channel: Option<&str>) -> (String, String, String) {
    let site_lc = site.unwrap_or("").to_lowercase();
    let channel = channel.unwrap_or("").trim();

    // Default: English top stories.
    if site_lc.is_empty() {
        return (
            "https://feeds.bbci.co.uk/news/rss.xml".to_string(),
            "https://www.bbc.co.uk/news".to_string(),
            "BBC News Top Stories".to_string(),
        );
    }

    // Simplified Chinese.
    if site_lc == "chinese" {
        if channel.is_empty() {
            return (
                "https://www.bbc.co.uk/zhongwen/simp/index.xml".to_string(),
                "https://www.bbc.com/zhongwen/simp".to_string(),
                "BBC News 中文网".to_string(),
            );
        } else {
            return (
                format!("https://www.bbc.co.uk/zhongwen/simp/{}/index.xml", channel),
                format!("https://www.bbc.com/zhongwen/simp/{}", channel),
                format!("BBC News 中文网 - {}", channel),
            );
        }
    }

    // Traditional Chinese.
    if site_lc == "traditionalchinese" {
        if channel.is_empty() {
            return (
                "https://www.bbc.co.uk/zhongwen/trad/index.xml".to_string(),
                "https://www.bbc.com/zhongwen/trad".to_string(),
                "BBC News 中文網".to_string(),
            );
        } else {
            return (
                format!("https://www.bbc.co.uk/zhongwen/trad/{}/index.xml", channel),
                format!("https://www.bbc.com/zhongwen/trad/{}", channel),
                format!("BBC News 中文網 - {}", channel),
            );
        }
    }

    // Generic English channels, mapping `world-asia` -> `world/asia`.
    let slug = site_lc.replace('-', "/");
    let feed_url = format!("https://feeds.bbci.co.uk/news/{}/rss.xml", slug);
    let home_url = format!("https://www.bbc.co.uk/news/{}", slug);
    let title = format!("BBC News {}", slug);
    (feed_url, home_url, title)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let site = ctx.param_str("site");
    let channel = ctx.param_str("channel");

    let (feed_url, home_url, fallback_title) = build_bbc_feed(site, channel);

    let fetcher = make_fetcher()?;
    let feed = fetcher.fetch_feed(&feed_url).await?;

    let feed_title = feed
        .title
        .as_ref()
        .map(|t| t.content.clone())
        .unwrap_or_else(|| fallback_title.clone());
    let feed_link = feed
        .links
        .get(0)
        .map(|l| l.href.clone())
        .unwrap_or_else(|| home_url.clone());
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
        description: Some(fallback_title),
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
pub const ROUTE_BBC_NEWS: Route = Route {
    meta: &META_BBC_NEWS,
    handler: handler_fn,
};
