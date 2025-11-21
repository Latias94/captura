//! AIBase (AI Base) related routes.
//!
//! Currently implemented:
//! - /aibase/news   AI news list via official JSON API.
//! - /aibase/daily  AI daily brief with full article content.
//!
//! The heavier discovery/product listing routes from RSSHub are not
//! implemented yet; this module focuses on news & daily reading flows.

pub mod daily;
pub mod news;
