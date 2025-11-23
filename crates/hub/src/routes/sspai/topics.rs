use crate::routes::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset, TimeZone};
use serde::Deserialize;

fn parse_unix_to_fixed(ts: i64) -> Option<DateTime<FixedOffset>> {
    let naive = chrono::NaiveDateTime::from_timestamp_opt(ts, 0)?;
    let offset = FixedOffset::east_opt(0)?;
    Some(offset.from_utc_datetime(&naive))
}

#[derive(Debug, Deserialize)]
struct TopicsResp {
    list: Vec<TopicItem>,
}

#[derive(Debug, Deserialize)]
struct TopicItem {
    id: i64,
    title: String,
    intro: String,
    banner: String,
    released_at: i64,
    author: TopicAuthor,
}

#[derive(Debug, Deserialize)]
struct TopicAuthor {
    nickname: String,
}

pub const META_SSPAI_TOPICS: RouteMeta = RouteMeta {
    hub_id: "sspai/topics",
    path: "/sspai/topics",
    categories: &["new-media"],
    example: "/sspai/topics",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["sspai.com/topics"],
        target: "/topics",
    }],
    name: "SSPAI Topics",
    maintainers: &["captura"],
    url: "https://sspai.com/topics",
    description: "少数派专题广场更新推送（专题本身而非专题内文章），对标 RSSHub /sspai/topics 路由。",
    default_view: Some("articles"),
};

pub async fn handler(_ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let api_url = "https://sspai.com/api/v1/topics?offset=0&limit=20&include_total=false";
    let client = captura_net::client_basic(None, None)
        .map_err(|e| Error::Network(format!("sspai client error: {}", e)))?;
    let resp = client
        .get(api_url)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{api_url} -> {e}")))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!("{api_url} -> http status {status}")));
    }

    let api: TopicsResp = resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("sspai topics json parse: {e}")))?;

    let mut items = Vec::new();
    for topic in api.list {
        let link = format!("https://sspai.com/topic/{}", topic.id);
        let banner_url = if topic.banner.starts_with("http") {
            topic.banner.clone()
        } else {
            format!("https://cdnfile.sspai.com/{}", topic.banner)
        };

        let description = format!(
            r#"<br><img src="{}" alt="Topic Cover" style="display:block;margin:0 auto;">{}<br>如有兴趣, 请复制链接订阅 <br><h3>https://rsshub.app/sspai/topic/{}</h3>"#,
            banner_url, topic.intro, topic.id
        );

        let pub_date = parse_unix_to_fixed(topic.released_at);

        items.push(HubItem {
            title: topic.title.trim().to_string(),
            description: Some(description),
            link: Some(link),
            author: Some(topic.author.nickname),
            pub_date,
            categories: Vec::new(),
        });
    }

    Ok(HubData {
        title: "少数派专题广场更新推送".to_string(),
        description: Some("仅推送新的专题（集合型而非具体文章）。".to_string()),
        link: Some("https://sspai.com/topics".to_string()),
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
pub const ROUTE_SSPAI_TOPICS: Route = Route {
    meta: &META_SSPAI_TOPICS,
    handler: handler_fn,
};
