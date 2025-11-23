use crate::routes::types::{Features, HubCtx, HubData, ParamMeta, Radar, Route, RouteMeta};
use captura_common::Error;
use captura_hub_macros::register_hub_route;

use super::fetch_cnblogs_list;

pub const META_CNBLOGS_AGGSITE: RouteMeta = RouteMeta {
    hub_id: "cnblogs/aggsite",
    path: "/cnblogs/aggsite/:kind",
    categories: &["programming"],
    example: "/cnblogs/aggsite/topdiggs",
    params: &[ParamMeta {
        name: "kind",
        description: "AggSite stream type: topdiggs (10-day recommended), topviews (10-day views), or headline.",
        default: Some("topdiggs"),
        options: &[
            ("topdiggs", "10-day recommended"),
            ("topviews", "10-day most viewed"),
            ("headline", "Homepage headline"),
        ],
    }],
    features: Features::basic(),
    radar: &[
        Radar {
            source: &["cnblogs.com/aggsite/topdiggs"],
            target: "/aggsite/topdiggs",
        },
        Radar {
            source: &["cnblogs.com/aggsite/topviews"],
            target: "/aggsite/topviews",
        },
        Radar {
            source: &["cnblogs.com/aggsite/headline"],
            target: "/aggsite/headline",
        },
    ],
    name: "博客园 AggSite 排行榜",
    maintainers: &["captura"],
    url: "https://www.cnblogs.com/aggsite/topdiggs",
    description: "cnblogs.com AggSite rankings (10-day recommended, 10-day most viewed, and homepage headline), aligned with RSSHub /cnblogs/aggsite routes.",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let kind = ctx.param_str("kind").unwrap_or("topdiggs");
    let limit = ctx.param_i64("limit").unwrap_or(40).max(1) as usize;

    let sub_path = match kind {
        "topviews" => "/aggsite/topviews",
        "headline" => "/aggsite/headline",
        _ => "/aggsite/topdiggs",
    };

    fetch_cnblogs_list(sub_path, limit).await
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_CNBLOGS_AGGSITE: Route = Route {
    meta: &META_CNBLOGS_AGGSITE,
    handler: handler_fn,
};
