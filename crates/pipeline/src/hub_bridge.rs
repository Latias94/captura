use captura_common::{NormalizedEntry, Result};
use captura_rules::hub::types::{HandlerCtx as HubHandlerCtx, HubData, HubHandler, HubResult};
use captura_rules::v1::{merge_rule_params_v1, RuleSpecV1};
use captura_storage::entity::feed;

fn find_builtin_handler(hub_id: &str) -> Option<&'static dyn HubHandler> {
    captura_rules::hub::registry::builtin_routes()
        .iter()
        .find(|r| r.meta.hub_id == hub_id)
        .map(|r| r.handler)
}

fn hub_result_to_entries(res: HubResult) -> Result<Vec<NormalizedEntry>> {
    match res {
        HubResult::Data(HubData { items, .. }) => {
            let mut entries = Vec::new();
            for item in items {
                let url = item.link.clone();
                entries.push(NormalizedEntry {
                    guid: url.clone(),
                    url,
                    title: Some(item.title),
                    summary: item.description.clone(),
                    content_html: item.description,
                    author: item.author,
                    published_at: item.pub_date.map(|d| d.with_timezone(&chrono::Utc)),
                    enclosures: Vec::new(),
                    extras: serde_json::json!({}),
                });
            }
            Ok(entries)
        }
    }
}

/// Try executing a built-in Hub handler for a given rule spec (mapped by rule id).
pub(crate) async fn execute_builtin_hub_for_rule(
    feed: &feed::Model,
    spec: &RuleSpecV1,
) -> Option<Result<Vec<NormalizedEntry>>> {
    let hub_id = if let Some(rest) = spec.id.strip_prefix("captura.route.") {
        rest.replace('.', "/")
    } else {
        return None;
    };

    let params = merge_rule_params_v1(spec, feed.rule_params_json.as_ref());
    let mut param_map = serde_json::Map::new();
    if let Some(val) = params {
        if let Some(obj) = val.as_object() {
            param_map = obj.clone();
        }
    }

    let mut ctx = HubHandlerCtx {
        hub_id: &hub_id,
        params: &param_map,
    };

    let handler = match find_builtin_handler(&hub_id) {
        Some(h) => h,
        None => return None,
    };

    let res = handler.handle(&mut ctx).await;
    Some(res.and_then(hub_result_to_entries))
}

/// Execute a Hub route by its id and parameters, returning `HubResult`.
pub async fn execute_hub_route(
    hub_id: &str,
    params: &serde_json::Map<String, serde_json::Value>,
) -> captura_common::Result<HubResult> {
    let handler = find_builtin_handler(hub_id)
        .ok_or_else(|| captura_common::Error::Config(format!("unknown hub route: {}", hub_id)))?;
    let mut ctx = HubHandlerCtx { hub_id, params };
    handler.handle(&mut ctx).await
}
