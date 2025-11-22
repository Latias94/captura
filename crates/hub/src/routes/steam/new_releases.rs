use crate::routes::types::{Features, HubCtx, HubData, Radar, Route, RouteMeta};
use captura_hub_macros::register_hub_route;

use super::search::run_search_with_params;

const QUERY_NEW_RELEASES: &str = "sort_by=Released_DESC&os=win&supportedlang=schinese";

pub const META_STEAM_NEW_RELEASES: RouteMeta = RouteMeta {
    hub_id: "steam/new-releases",
    path: "/steam/new-releases",
    categories: &["game"],
    example: "/steam/new-releases",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &[
            "store.steampowered.com",
            "store.steampowered.com/search",
            "store.steampowered.com/search/*",
        ],
        target: "/new-releases",
    }],
    name: "Steam - New Releases (Win, zh-CN)",
    maintainers: &["captura"],
    url: "https://store.steampowered.com/search/",
    description: "Steam Store new releases for Windows with Simplified Chinese support.",
    default_view: Some("games"),
};

pub async fn handler(_ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    run_search_with_params(QUERY_NEW_RELEASES).await
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_STEAM_NEW_RELEASES: Route = Route {
    meta: &META_STEAM_NEW_RELEASES,
    handler: handler_fn,
};
