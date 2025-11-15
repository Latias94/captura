//! Rules DSL crate.
//!
//! This crate exposes Rules DSL v1 schema (`v1` module),
//! matching `docs/rules-dsl.md`. Older minimal DSL support
//! has been removed in favour of the v1 design.

pub mod bilibili;
pub mod v1;

pub use v1::{parse_rule_v1, RuleSpecV1};
