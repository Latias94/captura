use crate::routes::types::{Features, HubCtx, HubData, ParamMeta, Radar, Route, RouteMeta};
use captura_common::Error;
use captura_hub_macros::register_hub_route;

use super::fetch_cnblogs_list;

pub const META_CNBLOGS_CATE: RouteMeta = RouteMeta {
    hub_id: "cnblogs/cate",
    path: "/cnblogs/cate/:type",
    categories: &["programming"],
    example: "/cnblogs/cate/go",
    params: &[ParamMeta {
        name: "type",
        description: "Category segment from cnblogs category URLs, e.g. go / python / java.",
        default: None,
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["cnblogs.com/cate/:type"],
        target: "/cate/:type",
    }],
    name: "博客园分类",
    maintainers: &["captura"],
    url: "https://www.cnblogs.com",
    description: "cnblogs.com category listings, aligned with RSSHub /cnblogs/cate/:type routes.",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let cat = ctx
        .param_str("type")
        .ok_or_else(|| Error::Config("cnblogs/cate: missing type parameter".to_string()))?;
    let limit = ctx.param_i64("limit").unwrap_or(40).max(1) as usize;

    let sub_path = format!("/cate/{}", cat);
    fetch_cnblogs_list(&sub_path, limit).await
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_CNBLOGS_CATE: Route = Route {
    meta: &META_CNBLOGS_CATE,
    handler: handler_fn,
};
