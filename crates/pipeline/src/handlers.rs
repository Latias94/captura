use captura_common::{NormalizedEntry, Result};
use captura_rules::v1::RuleSpecV1;
use captura_storage::entity::feed;

/// If a given rule has a corresponding Rust handler, execute it; otherwise return None so callers can fall back to DSL.
pub(crate) async fn execute_rust_handler_if_any(
    feed: &feed::Model,
    spec: &RuleSpecV1,
) -> Option<Result<Vec<NormalizedEntry>>> {
    // Currently only Hub handlers are used to execute built-in routes; no additional legacy handlers are wired.
    if let Some(res) = crate::hub_bridge::execute_builtin_hub_for_rule(feed, spec).await {
        return Some(res);
    }
    None
}
