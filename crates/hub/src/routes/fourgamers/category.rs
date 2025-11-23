use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_hub_macros::register_hub_route;

use super::util::{BASE_URL, enrich_items, fetch_by_category, fetch_latest};

pub const META_FOURGAMERS_CATEGORY: RouteMeta = RouteMeta {
    hub_id: "4gamers/category",
    path: "/4gamers/category/:category?",
    categories: &["game"],
    example: "/4gamers/category/352",
    params: &[ParamMeta {
        name: "category",
        description: "Category id from 4Gamers, omit for latest.",
        default: None,
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["www.4gamers.com.tw/news", "www.4gamers.com.tw/"],
        target: "/category/:category?",
    }],
    name: "4Gamers - 最新 / 分类",
    maintainers: &["captura"],
    url: "https://www.4gamers.com.tw/news",
    description: "4Gamers 最新消息或指定分类文章列表。",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let category = ctx.param_str("category").map(|s| s.to_string());
    let limit: usize = 30;

    let (title, items, link) = if let Some(cat) = category.as_deref() {
        let (cat_name, list) = fetch_by_category(cat, limit).await?;
        (
            format!("4Gamers - {}", cat_name),
            list,
            format!("{}/news/category/{}", BASE_URL, cat),
        )
    } else {
        let list: Vec<HubItem> = fetch_latest(limit).await?;
        (
            "4Gamers - 最新消息".to_string(),
            list,
            format!("{}/news", BASE_URL),
        )
    };

    let items = enrich_items(items).await;

    Ok(HubData {
        title,
        description: None,
        link: Some(link),
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
pub const ROUTE_FOURGAMERS_CATEGORY: Route = Route {
    meta: &META_FOURGAMERS_CATEGORY,
    handler: handler_fn,
};
