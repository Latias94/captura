use crate::hub::types::{Route, RouteMeta, RouteWrapper};
use once_cell::sync::Lazy;

// Collect all registered routes via `inventory`.
inventory::collect!(RouteWrapper);

static ROUTES: Lazy<Vec<Route>> =
    Lazy::new(|| inventory::iter::<RouteWrapper>().map(|w| w.0).collect());

/// Return all built-in Hub routes (static registry).
pub fn builtin_routes() -> &'static [Route] {
    ROUTES.as_slice()
}

/// Find a route meta by its Hub id, e.g. "github/trending".
pub fn find_route_meta(hub_id: &str) -> Option<&'static RouteMeta> {
    builtin_routes()
        .iter()
        .map(|r| r.meta)
        .find(|m| m.hub_id == hub_id)
}
