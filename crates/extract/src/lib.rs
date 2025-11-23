//! Content fetching / extraction and rule DSL crate.
//!
//! This crate provides reusable primitives for:
//! - Fetching HTML pages with basic HTTP options;
//! - Extracting main article content and title;
//! - Rules DSL v1 schema (`v1` module) and helpers;
//! - Stateless v1 JSON rule execution helpers;
//! - Common v1 runtime helpers (filters / templates / XPath subset).

mod entry;
pub mod v1;
mod v1_exec;
mod v1_runtime;

pub use entry::{
    EntryExtractConfig, ExtractResult, extract_from_html, fetch_and_extract_entry,
    fetch_and_extract_entry_dto,
};
pub use v1::{RuleSpecV1, parse_rule_v1};
pub use v1_exec::{RuleExecCtx, RuleExecHttpCtx, execute_json_v1_stateless};
pub use v1_runtime::{
    apply_description_template_v1, apply_rule_filters_v1, extract_html, json_get_path,
    xpath_to_css_like,
};
