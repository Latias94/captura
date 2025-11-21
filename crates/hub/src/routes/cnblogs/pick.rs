use crate::routes::types::{Features, HubCtx, HubData, Radar, Route, RouteMeta};
use captura_hub_macros::register_hub_route;

use super::fetch_cnblogs_list;

pub const META_CNBLOGS_PICK: RouteMeta = RouteMeta {
    hub_id: "cnblogs/pick",
    path: "/cnblogs/pick",
    categories: &["programming"],
    example: "/cnblogs/pick",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["cnblogs.com/pick"],
        target: "/pick",
    }],
    name: "博客园编辑推荐",
    maintainers: &["captura"],
    url: "https://www.cnblogs.com/pick",
    description: "cnblogs.com editor picks, aligned with RSSHub /cnblogs/pick route.",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let limit = ctx.param_i64("limit").unwrap_or(40).max(1) as usize;
    fetch_cnblogs_list("/pick", limit).await
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_CNBLOGS_PICK: Route = Route {
    meta: &META_CNBLOGS_PICK,
    handler: handler_fn,
};
