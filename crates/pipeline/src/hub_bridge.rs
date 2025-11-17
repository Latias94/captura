use captura_common::{NormalizedEntry, Result};
use captura_rules::routes::types::{HubCtx, HubData, Route};

fn find_builtin_route(hub_id: &str) -> Option<&'static Route> {
    captura_rules::routes::registry::builtin_routes()
        .iter()
        .find(|r| r.meta.hub_id == hub_id)
}

pub(crate) fn hub_data_to_entries(data: HubData) -> Result<Vec<NormalizedEntry>> {
    let mut entries = Vec::new();
    for item in data.items {
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

/// Execute a Hub route by its id and parameters, returning `HubData`.
pub async fn execute_hub_route(
    hub_id: &str,
    params: &serde_json::Map<String, serde_json::Value>,
) -> captura_common::Result<HubData> {
    let route = find_builtin_route(hub_id)
        .ok_or_else(|| captura_common::Error::Config(format!("unknown hub route: {}", hub_id)))?;
    let mut ctx = HubCtx { hub_id, params };
    (route.handler)(&mut ctx).await
}
