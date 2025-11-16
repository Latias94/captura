pub mod front;

use crate::hub::types::{RouteMeta, RouteRegistration};

pub const ROUTES: &[&RouteMeta] = &[&front::META_HN_FRONT];

pub const ROUTE_REGISTRATIONS: [RouteRegistration; 1] = [front::ROUTE_HN_FRONT];
