use crate::routes::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use crate::routes::util;
use captura_common::{Error, Result};
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

pub const META_MEITUAN_TECH: RouteMeta = RouteMeta {
    hub_id: "meituan/tech",
    path: "/meituan/tech",
    categories: &["programming"],
    example: "/meituan/tech",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["tech.meituan.com"],
        target: "/tech",
    }],
    name: "美团技术团队博客",
    maintainers: &["captura"],
    url: "https://tech.meituan.com/",
    description: "美团技术团队博客文章，对标 RSSHub /meituan/tech 路由。",
    default_view: Some("articles"),
};

pub async fn handler(_ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let feed_url = "https://tech.meituan.com/feed/";

    let fetcher = make_fetcher()?;
    let feed = fetcher.fetch_feed(feed_url).await?;

    let title = feed
        .title
        .as_ref()
        .map(|t| t.content.clone())
        .unwrap_or_else(|| "美团技术团队博客".to_string());
    let link = feed
        .links
        .get(0)
        .map(|l| l.href.clone())
        .unwrap_or_else(|| "https://tech.meituan.com/".to_string());
    let feed_desc = feed.description.as_ref().map(|d| d.content.clone());
    let image = feed
        .icon
        .as_ref()
        .map(|i| i.uri.clone())
        .or_else(|| feed.logo.as_ref().map(|i| i.uri.clone()));

    let mut items = Vec::new();

    let sel_content = Selector::parse("div.content")
        .map_err(|e| Error::Parse(format!("meituan: invalid content selector: {e}")))?;

    for entry in feed.entries {
        let item_title = entry
            .title
            .as_ref()
            .map(|t| t.content.clone())
            .unwrap_or_else(|| entry.id.clone());
        if item_title.trim().is_empty() {
            continue;
        }

        let link_url = entry.links.get(0).map(|l| l.href.clone());
        let mut description = None;

        if let Some(link_url) = &link_url {
            if let Ok(html) = util::get_html(link_url).await {
                let doc = Html::parse_document(&html);
                if let Some(el) = doc.select(&sel_content).next() {
                    let body = util::element_html(&el);
                    if !body.trim().is_empty() {
                        description = Some(body);
                    }
                }
            }
        }

        if description.is_none() {
            if let Some(body) = entry
                .content
                .as_ref()
                .and_then(|c| c.body.clone())
                .or_else(|| entry.summary.as_ref().map(|s| s.content.clone()))
            {
                if !body.trim().is_empty() {
                    description = Some(body);
                }
            }
        }

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

        let pub_date = entry.published.or(entry.updated).and_then(to_fixed_offset);
        let categories = entry
            .categories
            .iter()
            .map(|c| c.term.clone())
            .collect::<Vec<_>>();

        items.push(HubItem {
            title: item_title,
            description,
            link: link_url,
            author,
            pub_date,
            categories,
        });
    }

    Ok(HubData {
        title,
        description: feed_desc,
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
pub const ROUTE_MEITUAN_TECH: Route = Route {
    meta: &META_MEITUAN_TECH,
    handler: handler_fn,
};
