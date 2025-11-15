use crate::hub::types::{RouteMeta, RouteRegistration};
use crate::hub::{bilibili, github, hn, lobsters, medium, reuters, zhihu};

static BUILTIN_ROUTE_METAS: [&RouteMeta; 14] = [
    github::ROUTES[0],
    hn::ROUTES[0],
    lobsters::ROUTES[0],
    zhihu::ROUTES[0],
    reuters::ROUTES[0],
    medium::ROUTES[0],
    bilibili::ROUTES[0],
    bilibili::ROUTES[1],
    bilibili::ROUTES[2],
    bilibili::ROUTES[3],
    bilibili::ROUTES[4],
    bilibili::ROUTES[5],
    bilibili::ROUTES[6],
    bilibili::ROUTES[7],
];

/// Placeholder registry for built-in Hub routes metadata.
pub fn builtin_route_metas() -> &'static [&'static RouteMeta] {
    &BUILTIN_ROUTE_METAS
}

/// Full route registry (including handlers); currently no concrete handlers are wired here.
pub fn builtin_routes() -> &'static [RouteRegistration] {
    &[]
}

/// Find a route meta by its Hub id, e.g. "github/trending".
pub fn find_route_meta(hub_id: &str) -> Option<&'static RouteMeta> {
    builtin_route_metas()
        .iter()
        .copied()
        .find(|m| m.hub_id == hub_id)
}
