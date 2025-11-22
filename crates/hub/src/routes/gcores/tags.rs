use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_hub_macros::register_hub_route;

use super::util::{self as gcores_util, BASE_URL};

pub const META_GCORES_TAGS: RouteMeta = RouteMeta {
    hub_id: "gcores/tags",
    path: "/gcores/tags/:id/:tab?",
    categories: &["game"],
    example: "/gcores/tags/1/articles",
    params: &[
        ParamMeta {
            name: "id",
            description: "标签 ID，可在标签页 URL 中找到。",
            default: None,
            options: &[],
        },
        ParamMeta {
            name: "tab",
            description: "类型：radios/articles/news/videos，留空为全部。",
            default: None,
            options: &[
                ("radios", "播客"),
                ("articles", "文章"),
                ("news", "资讯"),
                ("videos", "视频"),
            ],
        },
    ],
    features: Features::basic(),
    radar: &[Radar {
        source: &["www.gcores.com/tags/:id/originals"],
        target: "/gcores/tags/:id/:tab?",
    }],
    name: "机核 - 标签",
    maintainers: &["captura"],
    url: "https://www.gcores.com",
    description: "机核标签聚合内容。",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let id = ctx
        .param_str("id")
        .ok_or_else(|| captura_common::Error::Config("gcores/tags: id is required".to_string()))?;
    let tab = ctx.param_str("tab");
    let limit = 30usize;

    let target_url = if let Some(t) = tab {
        format!("{}/tags/{}/originals?tab={}", BASE_URL, id, t)
    } else {
        format!("{}/tags/{}/originals", BASE_URL, id)
    };
    let api_tab = tab.unwrap_or("originals");
    let api_url = format!("{}/gapi/v1/tags/{}/{}", BASE_URL, id, api_tab);

    let mut query = serde_json::Map::new();
    query.insert("page[limit]".to_string(), serde_json::json!(limit));
    query.insert("sort".to_string(), serde_json::json!("-published-at"));
    query.insert(
        "include".to_string(),
        serde_json::json!("category,user,media"),
    );
    query.insert("filter[list-all]".to_string(), serde_json::json!(1));
    if matches!(tab, Some("news")) {
        query.insert("filter[is-news]".to_string(), serde_json::json!(1));
    } else {
        query.insert("filter[is-news]".to_string(), serde_json::json!(0));
    }

    let default_view = match tab {
        Some("radios") => Some("audios"),
        Some("videos") => Some("videos"),
        _ => Some("articles"),
    };

    let (title, description, link, language, items): (
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Vec<HubItem>,
    ) = gcores_util::process_items(limit, Some(&query), &api_url, &target_url, default_view)
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
pub const ROUTE_GCORES_TAGS: Route = Route {
    meta: &META_GCORES_TAGS,
    handler: handler_fn,
};
