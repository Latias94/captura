//! Steam Store / Community related routes.
//!
//! - /steam/search/:params        Store search (new releases, discounts, etc.).
//! - /steam/new-releases         Shortcut for Windows zh-CN new releases.
//! - /steam/discounts            Shortcut for Windows zh-CN discounts.

pub mod discounts;
pub mod new_releases;
pub mod search;
