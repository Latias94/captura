use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset, TimeZone};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct V2exTopicDetail {
    id: i64,
    title: String,
    url: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct V2exReply {
    id: i64,
    content: String,
    content_rendered: String,
    created: i64,
    member: V2exMember,
}

#[derive(Debug, Deserialize)]
struct V2exMember {
    username: String,
}

const V2EX_API_BASE: &str = "https://www.v2ex.com/api";

fn parse_unix_to_fixed(ts: i64) -> Option<DateTime<FixedOffset>> {
    let naive = chrono::NaiveDateTime::from_timestamp_opt(ts, 0)?;
    let offset = FixedOffset::east_opt(0)?;
    Some(offset.from_utc_datetime(&naive))
}

pub const META_V2EX_POST: RouteMeta = RouteMeta {
    hub_id: "v2ex/post",
    path: "/v2ex/post/:postid",
    categories: &["bbs"],
    example: "/v2ex/post/584403",
    params: &[ParamMeta {
        name: "postid",
        description: "V2EX post id from /t/:postid URL",
        default: None,
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["www.v2ex.com/t/:postid", "v2ex.com/t/:postid"],
        target: "/t/:postid",
    }],
    name: "V2EX Post",
    maintainers: &["captura"],
    url: "https://www.v2ex.com/",
    description: "Single V2EX topic with replies (API based, inspired by RSSHub v2ex/post).",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let postid = ctx.param_str("postid").unwrap_or("");
    if postid.is_empty() {
        return Err(captura_common::Error::Config(
            "postid is required for v2ex/post".into(),
        ));
    }
    let page_url = format!("https://www.v2ex.com/t/{}", postid);

    // Topic detail
    let topic_url = format!("{}/topics/show.json?id={}", V2EX_API_BASE, postid);
    let topics: Vec<V2exTopicDetail> = util::get_json(&topic_url).await?;
    let topic = match topics.into_iter().next() {
        Some(t) => t,
        None => {
            return Ok(HubData {
                title: format!("V2EX-{}", postid),
                link: Some(page_url),
                description: Some(String::new()),
                image: None,
                language: None,
                items: Vec::new(),
                allow_empty: true,
            });
        }
    };

    // Replies
    let replies_url = format!("{}/replies/show.json?topic_id={}", V2EX_API_BASE, postid);
    let replies: Vec<V2exReply> = util::get_json(&replies_url).await?;

    let mut items = Vec::new();
    for (idx, r) in replies.into_iter().enumerate() {
        let title = format!("#{} {}", idx + 1, r.content);
        let link = format!("{}#r_{}", page_url, r.id);
        let pub_date = parse_unix_to_fixed(r.created);

        items.push(HubItem {
            title,
            description: Some(r.content_rendered),
            link: Some(link),
            author: Some(r.member.username),
            pub_date,
            categories: Vec::new(),
        });
    }

    Ok(HubData {
        title: format!("V2EX-{}", topic.title),
        link: Some(page_url),
        description: Some(topic.content),
        image: None,
        language: None,
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_V2EX_POST: Route = Route {
    meta: &META_V2EX_POST,
    handler: handler_fn,
};
