use crate::routes::types::{Features, HubCtx, HubData, Radar, Route, RouteMeta};
use captura_hub_macros::register_hub_route;

use super::search::run_search_with_params;

const QUERY_DISCOUNTS: &str = "sort_by=Released_DESC&specials=1&os=win&supportedlang=schinese";

pub const META_STEAM_DISCOUNTS: RouteMeta = RouteMeta {
    hub_id: "steam/discounts",
    path: "/steam/discounts",
    categories: &["game"],
    example: "/steam/discounts",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &[
            "store.steampowered.com",
            "store.steampowered.com/search",
            "store.steampowered.com/search/*",
        ],
        target: "/discounts",
    }],
    name: "Steam - Discounts (Win, zh-CN)",
    maintainers: &["captura"],
    url: "https://store.steampowered.com/search/",
    description: "Steam Store discounted games for Windows with Simplified Chinese support.",
    default_view: Some("games"),
};

pub async fn handler(_ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    run_search_with_params(QUERY_DISCOUNTS).await
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_STEAM_DISCOUNTS: Route = Route {
    meta: &META_STEAM_DISCOUNTS,
    handler: handler_fn,
};
