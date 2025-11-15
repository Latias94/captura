use captura_common::{NormalizedEntry, Result};
use captura_rules::v1::RuleSpecV1;
use captura_storage::entity::feed;

/// 若给定规则存在对应的 Rust handler，则执行之；否则返回 None 让调用方走 DSL 路径。
pub(crate) async fn execute_rust_handler_if_any(
    feed: &feed::Model,
    spec: &RuleSpecV1,
) -> Option<Result<Vec<NormalizedEntry>>> {
    // 目前仅通过 Hub handler 执行内建路由；没有额外 legacy handler。
    if let Some(res) = crate::hub_bridge::execute_builtin_hub_for_rule(feed, spec).await {
        return Some(res);
    }
    None
}
