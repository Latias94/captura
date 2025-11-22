//! The Guardian related routes.
//!
//! Currently implemented:
//! - /guardian/rss/:section?  Generic section RSS, default to world news.
//! - /guardian/todayinfocus     Today in Focus podcast feed.

pub mod rss;
pub mod todayinfocus;
