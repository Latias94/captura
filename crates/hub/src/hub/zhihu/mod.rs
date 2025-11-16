pub mod hotlist;

use crate::hub::types::{RouteMeta, RouteRegistration};

pub const ROUTES: &[&RouteMeta] = &[&hotlist::META_ZHIHU_HOTLIST];

pub const ROUTE_REGISTRATIONS: [RouteRegistration; 1] = [hotlist::ROUTE_ZHIHU_HOTLIST];
