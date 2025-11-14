pub mod trending;

use crate::types::RouteMeta;

pub const ROUTES: &[&RouteMeta] = &[&trending::META_GITHUB_TRENDING];
