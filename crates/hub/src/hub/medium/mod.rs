pub mod tag;

use crate::hub::types::{RouteMeta, RouteRegistration};

pub const ROUTES: &[&RouteMeta] = &[&tag::META_MEDIUM_TAG];

pub const ROUTE_REGISTRATIONS: [RouteRegistration; 1] = [tag::ROUTE_MEDIUM_TAG];
