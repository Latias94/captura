use crate::routes::types::{Features, HubCtx, HubData, Radar, Route, RouteMeta};
use captura_hub_macros::register_hub_route;

use super::{fetch_cnbeta, CnbetaKind, ROOT_URL};

pub const META_CNBETA_INDEX: RouteMeta = RouteMeta {
    hub_id: "cnbeta",
    path: "/cnbeta",
    categories: &["new-media"],
    example: "/cnbeta",
    params: &[],
    features: Features::with_anti_crawler(),
    radar: &[Radar {
        source: &["cnbeta.com.tw/"],
        target: "/",
    }],
    name: "cnBeta 头条资讯",
    maintainers: &["captura"],
    url: "https://www.cnbeta.com.tw",
    description:
        "cnBeta.COM homepage stream (headlines and latest articles), aligned with RSSHub /cnbeta route.",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let limit = ctx.param_i64("limit").unwrap_or(60).max(1) as usize;

    let (items, title, description) = fetch_cnbeta(CnbetaKind::Index, limit).await?;

    Ok(HubData {
        title,
        description,
        link: Some(ROOT_URL.to_string()),
        image: None,
        language: Some("zh-TW".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_CNBETA_INDEX: Route = Route {
    meta: &META_CNBETA_INDEX,
    handler: handler_fn,
};
