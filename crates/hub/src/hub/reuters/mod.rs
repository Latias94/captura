pub mod top;

use crate::hub::types::{RouteMeta, RouteRegistration};

pub const ROUTES: &[&RouteMeta] = &[&top::META_REUTERS_TOP];

pub const ROUTE_REGISTRATIONS: [RouteRegistration; 1] = [top::ROUTE_REUTERS_TOP];
