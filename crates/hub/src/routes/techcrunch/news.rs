use crate::routes::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeZone};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct TcPost {
    id: i64,
    date_gmt: String,
    link: String,
    title: TcRendered,
    content: TcRendered,
}

#[derive(Debug, Deserialize)]
struct TcRendered {
    rendered: String,
}

fn parse_date_gmt(s: &str) -> Option<DateTime<FixedOffset>> {
    let naive = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").ok()?;
    let offset = FixedOffset::east_opt(0)?;
    Some(offset.from_utc_datetime(&naive))
}

pub const META_TECHCRUNCH_NEWS: RouteMeta = RouteMeta {
    hub_id: "techcrunch/news",
    path: "/techcrunch/news",
    categories: &["technology"],
    example: "/techcrunch/news",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["techcrunch.com/"],
        target: "/news",
    }],
    name: "TechCrunch News",
    maintainers: &["captura"],
    url: "https://techcrunch.com/",
    description: "TechCrunch 最新科技与创业新闻，参考 RSSHub techcrunch/news 实现。",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let api_url = "https://techcrunch.com/wp-json/wp/v2/posts";
    let client = captura_net::client_basic(None, None)
        .map_err(|e| Error::Network(format!("techcrunch client error: {}", e)))?;
    let resp = client
        .get(api_url)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{api_url} -> {e}")))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!("{api_url} -> http status {status}")));
    }
    let posts: Vec<TcPost> = resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("techcrunch posts json parse: {e}")))?;

    let limit = ctx.param_i64("limit").unwrap_or(20).max(1) as usize;
    let mut items = Vec::new();

    for post in posts.into_iter().take(limit) {
        let title = post.title.rendered.trim().to_string();
        if title.is_empty() {
            continue;
        }

        let description = Some(post.content.rendered.clone());
        let link = Some(post.link.clone());
        let pub_date = parse_date_gmt(&post.date_gmt);

        items.push(HubItem {
            title,
            description,
            link,
            author: None,
            pub_date,
            categories: Vec::new(),
        });
    }

    Ok(HubData {
        title: "TechCrunch".to_string(),
        description: Some("TechCrunch 新闻：覆盖科技、创业公司和风投动态。".to_string()),
        link: Some("https://techcrunch.com/".to_string()),
        image: None,
        language: None,
        items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_TECHCRUNCH_NEWS: Route = Route {
    meta: &META_TECHCRUNCH_NEWS,
    handler: handler_fn,
};
