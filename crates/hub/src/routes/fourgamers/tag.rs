use crate::routes::types::{Features, HubCtx, HubData, ParamMeta, Radar, Route, RouteMeta};
use captura_hub_macros::register_hub_route;

use super::util::{enrich_items, fetch_by_tag, BASE_URL};

pub const META_FOURGAMERS_TAG: RouteMeta = RouteMeta {
    hub_id: "4gamers/tag",
    path: "/4gamers/tag/:tag",
    categories: &["game"],
    example: "/4gamers/tag/限時免費",
    params: &[ParamMeta {
        name: "tag",
        description: "标签名，可在 4Gamers 标签 URL 中找到，例如 限時免費。",
        default: None,
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["www.4gamers.com.tw/news/tag/:tag"],
        target: "/tag/:tag",
    }],
    name: "4Gamers - 标签",
    maintainers: &["captura"],
    url: "https://www.4gamers.com.tw/news",
    description: "4Gamers 指定标签文章列表，例如「限時免費」。",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let tag_owned = ctx
        .param_str("tag")
        .map(|s| s.to_string())
        .unwrap_or_else(|| "限時免費".to_string()); // default hot tag
    let tag = tag_owned.as_str();
    let limit: usize = 30;

    let list = fetch_by_tag(tag, limit).await?;
    let items = enrich_items(list).await;

    Ok(HubData {
        title: format!("4Gamers - #{}", tag),
        description: None,
        link: Some(format!("{}/news/tag/{}", BASE_URL, tag)),
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
pub const ROUTE_FOURGAMERS_TAG: Route = Route {
    meta: &META_FOURGAMERS_TAG,
    handler: handler_fn,
};
