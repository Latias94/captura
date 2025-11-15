pub mod front;

use crate::hub::types::RouteMeta;

pub const ROUTES: &[&RouteMeta] = &[&front::META_LOBSTERS_FRONT];
