use crate::routes::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use captura_hub_macros::register_hub_route;

use super::util::{self as gcores_util, BASE_URL};

pub const META_GCORES_NEWS: RouteMeta = RouteMeta {
    hub_id: "gcores/news",
    path: "/gcores/news",
    categories: &["game"],
    example: "/gcores/news",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["www.gcores.com/news"],
        target: "/gcores/news",
    }],
    name: "机核 - 资讯",
    maintainers: &["captura"],
    url: "https://www.gcores.com",
    description: "机核网资讯流。",
    default_view: Some("articles"),
};

pub async fn handler(_ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let target_url = format!("{}/news", BASE_URL);
    let api_url = format!("{}/gapi/v1/articles", BASE_URL);

    let mut query = serde_json::Map::new();
    query.insert("page[limit]".to_string(), serde_json::json!(30));
    query.insert("sort".to_string(), serde_json::json!("-published-at"));
    query.insert(
        "include".to_string(),
        serde_json::json!("category,user,media"),
    );
    query.insert("filter[list-all]".to_string(), serde_json::json!(1));
    query.insert("filter[is-news]".to_string(), serde_json::json!(1));

    let (title, description, link, language, items): (
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Vec<HubItem>,
    ) = gcores_util::process_items(30, Some(&query), &api_url, &target_url, Some("articles"))
        .await?;

    Ok(HubData {
        title,
        description,
        link,
        image: None,
        language,
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_GCORES_NEWS: Route = Route {
    meta: &META_GCORES_NEWS,
    handler: handler_fn,
};
