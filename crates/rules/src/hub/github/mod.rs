pub mod trending;

use crate::hub::types::RouteMeta;

pub const ROUTES: &[&RouteMeta] = &[&trending::META_GITHUB_TRENDING];
