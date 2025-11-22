use crate::routes::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use captura_hub_macros::register_hub_route;

use super::util::{self as gcores_util, BASE_URL};

pub const META_GCORES_PROGRAM_PREVIEWS: RouteMeta = RouteMeta {
    hub_id: "gcores/radios/preview",
    path: "/gcores/radios/preview",
    categories: &["game"],
    example: "/gcores/radios/preview",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["www.gcores.com/radios/preview"],
        target: "/gcores/radios/preview",
    }],
    name: "机核 - 节目预告",
    maintainers: &["captura"],
    url: "https://www.gcores.com",
    description: "机核电台与视频节目预告。",
    default_view: Some("notifications"),
};

pub async fn handler(_ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let limit = 30usize;
    let target_url = format!("{}/radios/preview", BASE_URL);
    let api_url = format!("{}/gapi/v1/program-previews", BASE_URL);

    let mut query = serde_json::Map::new();
    query.insert(
        "include".to_string(),
        serde_json::json!("radio.djs,video.djs,radio.category,video.category"),
    );
    query.insert("page[limit]".to_string(), serde_json::json!(limit));

    let (title, description, link, language, items): (
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Vec<HubItem>,
    ) = gcores_util::process_items(
        limit,
        Some(&query),
        &api_url,
        &target_url,
        Some("notifications"),
    )
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
pub const ROUTE_GCORES_PROGRAM_PREVIEWS: Route = Route {
    meta: &META_GCORES_PROGRAM_PREVIEWS,
    handler: handler_fn,
};
