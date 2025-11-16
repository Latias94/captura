pub mod trending;

use crate::hub::types::{RouteMeta, RouteRegistration};

pub const ROUTES: &[&RouteMeta] = &[&trending::META_GITHUB_TRENDING];

pub const ROUTE_REGISTRATIONS: [RouteRegistration; 1] = [trending::ROUTE_GITHUB_TRENDING];
