//! Rules + Hub crate.
//!
//! This crate exposes:
//! - Rules DSL v1 schema (`v1` module) and helpers;
//! - Built-in rules implementations for specific sites (e.g. Bilibili);
//! - Hub route metadata (`routes` module), mirroring RSSHub-style routes.

pub mod routes;
pub mod v1;

pub use v1::{parse_rule_v1, RuleSpecV1};
