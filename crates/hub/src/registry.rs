use crate::types::{RouteMeta, RouteRegistration};
use crate::{github, hn, lobsters, medium, reuters, zhihu};

static BUILTIN_ROUTE_METAS: [&RouteMeta; 6] = [
    github::ROUTES[0],
    hn::ROUTES[0],
    lobsters::ROUTES[0],
    zhihu::ROUTES[0],
    reuters::ROUTES[0],
    medium::ROUTES[0],
];

/// Placeholder registry for built-in Hub routes metadata.
pub fn builtin_route_metas() -> &'static [&'static RouteMeta] {
    &BUILTIN_ROUTE_METAS
}

/// 完整的 Route 注册表（包含 handler），当前尚未接入具体 handler。
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
