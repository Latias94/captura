use captura_common::{NormalizedEntry, Result};
use captura_rules::v1::RuleSpecV1;
use captura_storage::entity::feed;
use serde_json::Value as JsonValue;

/// Handler 上下文：暴露当前订阅与规则信息，以及合并后的规则参数。
/// 后续可以继续扩展（HTTP 客户端、缓存等）。
pub(crate) struct HandlerCtx<'a> {
    pub feed: &'a feed::Model,
    pub spec: &'a RuleSpecV1,
    pub params: Option<JsonValue>,
}

impl<'a> HandlerCtx<'a> {
    pub fn new(feed: &'a feed::Model, spec: &'a RuleSpecV1) -> Self {
        let params =
            crate::rules_engine::merge_rule_params_v1(spec, feed.rule_params_json.as_ref());
        Self { feed, spec, params }
    }
}

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
