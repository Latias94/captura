use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_hub_macros::register_hub_route;

use super::util::{self as gcores_util, BASE_URL};

pub const META_GCORES_TOPICS_RECOMMEND: RouteMeta = RouteMeta {
    hub_id: "gcores/topics/recommend",
    path: "/gcores/topics/:id?/recommend",
    categories: &["game"],
    example: "/gcores/topics/recommend",
    params: &[ParamMeta {
        name: "id",
        description: "小组 ID，可在对应小组页 URL 中找到，留空为首页推荐。",
        default: None,
        options: &[],
    }],
    features: Features::basic(),
    radar: &[
        Radar {
            source: &["www.gcores.com/topics/home"],
            target: "/gcores/topics/recommend",
        },
        Radar {
            source: &["www.gcores.com/topics/:id"],
            target: "/gcores/topics/:id/recommend",
        },
    ],
    name: "机核 - 机组推荐",
    maintainers: &["captura"],
    url: "https://www.gcores.com",
    description: "机核机组推荐列表。",
    default_view: Some("social_media"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let id = ctx.param_str("id").ok_or_else(|| {
        captura_common::Error::Config("gcores/topics: id is required".to_string())
    })?;
    let limit = 30usize;

    let target_url = format!("{}/topics/{}", BASE_URL, id);
    let api_url = format!("{}/gapi/v1/topics/{}/recommend", BASE_URL, id);

    let mut query = serde_json::Map::new();
    query.insert("page[limit]".to_string(), serde_json::json!(limit));
    query.insert(
        "include".to_string(),
        serde_json::json!("talk,talk.topic,talk.user"),
    );
    query.insert("talk-include".to_string(), serde_json::json!("topic,user"));

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
        Some("social_media"),
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
pub const ROUTE_GCORES_TOPICS_RECOMMEND: Route = Route {
    meta: &META_GCORES_TOPICS_RECOMMEND,
    handler: handler_fn,
};
