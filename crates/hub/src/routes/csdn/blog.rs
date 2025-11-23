use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::Error;
use captura_fetcher::{FetchOptions, HttpFetcher};
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset, Utc};
use scraper::{Html, Selector};

fn make_fetcher() -> captura_common::Result<HttpFetcher> {
    HttpFetcher::new(FetchOptions::default())
}

fn to_fixed_offset(dt: DateTime<Utc>) -> Option<DateTime<FixedOffset>> {
    FixedOffset::east_opt(0).map(|offset| dt.with_timezone(&offset))
}

pub const META_CSDN_BLOG: RouteMeta = RouteMeta {
    hub_id: "csdn/blog",
    path: "/csdn/blog/:user",
    categories: &["programming"],
    example: "/csdn/blog/csdngeeknews",
    params: &[ParamMeta {
        name: "user",
        description: "CSDN blog username, taken from https://blog.csdn.net/{user}/ URLs.",
        default: None,
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["blog.csdn.net/:user"],
        target: "/blog/:user",
    }],
    name: "CSDN 博客",
    maintainers: &["captura"],
    url: "https://blog.csdn.net",
    description: "CSDN user blog feed based on the official RSS endpoints at rss.csdn.net, with optional full content extracted from article pages.",
    default_view: Some("articles"),
};

fn extract_content(html: &str) -> Option<String> {
    let doc = Html::parse_document(html);
    let sel = Selector::parse("#content_views").ok()?;
    let el = doc.select(&sel).next()?;
    let body = util::element_html(&el);
    if body.trim().is_empty() {
        None
    } else {
        Some(body)
    }
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let user = ctx
        .param_str("user")
        .ok_or_else(|| Error::Config("csdn/blog: missing user parameter".to_string()))?;
    let limit = ctx.param_i64("limit").unwrap_or(50).max(1) as usize;

    let root_url = "https://rss.csdn.net";
    let blog_url = format!("{}/{}", root_url, user);
    let rss_url = format!("{}/rss/map", blog_url);

    let fetcher = make_fetcher()?;
    let feed = fetcher.fetch_feed(&rss_url).await?;

    let feed_title = feed
        .title
        .as_ref()
        .map(|t| t.content.clone())
        .unwrap_or_else(|| format!("CSDN - {}", user));
    let feed_link = format!("https://blog.csdn.net/{}/", user);

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
                if let Some(full) = extract_content(&html) {
                    description = Some(full);
                }
            }
        }

        let pub_date = entry.published.or(entry.updated).and_then(to_fixed_offset);
        let author = if entry.authors.is_empty() {
            Some(user.to_string())
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
        title: format!("{} - CSDN博客", feed_title),
        description: Some(format!("CSDN blog feed for user '{}'.", user)),
        link: Some(feed_link),
        image: None,
        language: Some("zh-CN".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_CSDN_BLOG: Route = Route {
    meta: &META_CSDN_BLOG,
    handler: handler_fn,
};
