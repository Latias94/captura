pub mod hotlist;

use crate::hub::types::RouteMeta;

pub const ROUTES: &[&RouteMeta] = &[&hotlist::META_ZHIHU_HOTLIST];
