use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset, TimeZone};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct V2exTopic {
    id: i64,
    title: String,
    url: String,
    content: String,
    content_rendered: String,
    created: i64,
    replies: i64,
    member: V2exMember,
    node: V2exNode,
}

#[derive(Debug, Deserialize)]
struct V2exMember {
    username: String,
}

#[derive(Debug, Deserialize)]
struct V2exNode {
    title: String,
}

const V2EX_API_BASE: &str = "https://www.v2ex.com/api";

pub const META_V2EX_TOPICS: RouteMeta = RouteMeta {
    hub_id: "v2ex/topics",
    path: "/v2ex/topics/:type",
    categories: &["bbs"],
    example: "/v2ex/topics/latest",
    params: &[ParamMeta {
        name: "type",
        description: "topic type: hot / latest",
        default: Some("hot"),
        options: &[("hot", "Hot topics"), ("latest", "Latest topics")],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["www.v2ex.com"],
        target: "/",
    }],
    name: "V2EX Topics",
    maintainers: &["captura"],
    url: "https://www.v2ex.com/",
    description: "V2EX hot / latest topics (JSON API based, inspired by RSSHub v2ex/topics).",
    default_view: Some("articles"),
};

fn parse_unix_to_fixed(ts: i64) -> Option<DateTime<FixedOffset>> {
    let naive = chrono::NaiveDateTime::from_timestamp_opt(ts, 0)?;
    let offset = FixedOffset::east_opt(0)?;
    Some(offset.from_utc_datetime(&naive))
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let typ = ctx.param_str("type").unwrap_or("hot");
    let endpoint = match typ {
        "latest" => "latest",
        _ => "hot",
    };

    let url = format!("{}/topics/{}.json", V2EX_API_BASE, endpoint);
    let topics: Vec<V2exTopic> = crate::routes::util::get_json(&url).await?;

    let title_suffix = if endpoint == "hot" { "Hot" } else { "Latest" };
    let feed_title = format!("V2EX - {}", title_suffix);

    let mut items = Vec::new();
    for t in topics {
        let desc_html = format!("{}: {}", t.member.username, t.content_rendered);
        let pub_date = parse_unix_to_fixed(t.created);
        let categories = vec![t.node.title.clone()];
        items.push(HubItem {
            title: t.title.clone(),
            description: Some(desc_html),
            link: Some(t.url.clone()),
            author: Some(t.member.username.clone()),
            pub_date,
            categories,
        });
    }

    Ok(HubData {
        title: feed_title.clone(),
        description: Some(feed_title),
        link: Some("https://www.v2ex.com/".to_string()),
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
pub const ROUTE_V2EX_TOPICS: Route = Route {
    meta: &META_V2EX_TOPICS,
    handler: handler_fn,
};
