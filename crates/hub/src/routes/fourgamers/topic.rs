use crate::routes::types::{Features, HubCtx, HubData, ParamMeta, Radar, Route, RouteMeta};
use captura_hub_macros::register_hub_route;

use super::util::{BASE_URL, enrich_items, fetch_by_topic};

pub const META_FOURGAMERS_TOPIC: RouteMeta = RouteMeta {
    hub_id: "4gamers/topic",
    path: "/4gamers/topic/:topic",
    categories: &["game"],
    example: "/4gamers/topic/gentlemen-topic",
    params: &[ParamMeta {
        name: "topic",
        description: "主题 slug，可在 4Gamers 首页顶部主题入口的 URL 中找到。",
        default: None,
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["www.4gamers.com.tw/news/option-cfg/:topic"],
        target: "/topic/:topic",
    }],
    name: "4Gamers - 主題",
    maintainers: &["captura"],
    url: "https://www.4gamers.com.tw/news",
    description: "4Gamers 指定主题文章列表。",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let topic_owned = ctx
        .param_str("topic")
        .map(|s| s.to_string())
        .ok_or_else(|| {
            captura_common::Error::Config("4gamers/topic: topic is required".to_string())
        })?;
    let topic = topic_owned.as_str();
    let limit: usize = 30;

    let list = fetch_by_topic(topic, limit).await?;
    let items = enrich_items(list).await;

    Ok(HubData {
        title: format!("4Gamers - {}", topic),
        description: None,
        link: Some(format!("{}/news/option-cfg/{}", BASE_URL, topic)),
        image: None,
        language: Some("zh-TW".to_string()),
        items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_FOURGAMERS_TOPIC: Route = Route {
    meta: &META_FOURGAMERS_TOPIC,
    handler: handler_fn,
};
