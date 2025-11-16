//! Rules + Hub crate.
//!
//! This crate exposes:
//! - Rules DSL v1 schema (`v1` module) and helpers;
//! - Built-in rules implementations for specific sites (e.g. Bilibili);
//! - Hub route metadata (`hub` module), mirroring RSSHub-style routes.

pub mod hub;
pub mod v1;

// For ergonomics, expose a top-level `bilibili` module that re-exports
// the rule implementations living under `hub::bilibili::rules`.
pub mod bilibili {
    pub use crate::hub::bilibili::rules::*;
}

pub use v1::{parse_rule_v1, RuleSpecV1};
