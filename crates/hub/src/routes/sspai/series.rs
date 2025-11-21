use crate::routes::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use serde_json::Value;

pub const META_SSPAI_SERIES: RouteMeta = RouteMeta {
    hub_id: "sspai/series",
    path: "/sspai/series",
    categories: &["new-media"],
    example: "/sspai/series",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["sspai.com/series"],
        target: "/series",
    }],
    name: "SSPAI Latest Paid Series",
    maintainers: &["captura"],
    url: "https://sspai.com/series",
    description:
        "少数派最新上架付费专栏，仅作更新提醒，不含付费正文，对标 RSSHub /sspai/series 路由。",
    default_view: Some("articles"),
};

pub async fn handler(_ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let client = captura_net::client_basic(None, None)
        .map_err(|e| Error::Network(format!("sspai client error: {}", e)))?;

    let api_url = "https://sspai.com/api/v1/series/tag/all/get";
    let resp = client
        .get(api_url)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{api_url} -> {e}")))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!("{api_url} -> http status {status}")));
    }
    let text = resp
        .text()
        .await
        .map_err(|e| Error::Parse(format!("sspai series tag text: {e}")))?;
    let root: Value = serde_json::from_str(&text)
        .map_err(|e| Error::Parse(format!("sspai series tag json parse: {e}")))?;

    let data = root
        .get("data")
        .and_then(|v| v.as_array())
        .ok_or_else(|| Error::Parse("sspai series tag json parse: data is not array".into()))?;

    let mut items = Vec::new();
    for category in data {
        if let Some(children) = category.get("children").and_then(|v| v.as_array()) {
            for child in children {
                let sell_status = child
                    .get("sell_status")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if !sell_status {
                    continue;
                }

                let id = match child.get("id").and_then(|v| v.as_i64()) {
                    Some(v) => v,
                    None => continue,
                };

                let title_raw = child
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if title_raw.is_empty() {
                    continue;
                }

                let price = child.get("price").and_then(|v| v.as_i64()).unwrap_or(0);
                let price_yuan = price as f64 / 100.0;
                let title = format!("￥{price_yuan:.2} - {title_raw}");

                let banner = child.get("banner").and_then(|v| v.as_str()).unwrap_or("");
                let banner_url = if banner.starts_with("http") {
                    banner.to_string()
                } else {
                    format!("https://cdn.sspai.com/{}", banner)
                };

                let desc = child
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                let author_name = child
                    .get("author")
                    .and_then(|v| v.get("nickname"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("少数派");

                let description = format!(
                    r#"<img src="{banner}" alt="Series Banner" style="max-width:100%;">{intro}"#,
                    banner = banner_url,
                    intro = desc
                );

                let link = format!("https://sspai.com/series/{}", id);

                items.push(HubItem {
                    title,
                    description: Some(description),
                    link: Some(link),
                    author: Some(author_name.to_string()),
                    pub_date: None,
                    categories: Vec::new(),
                });
            }
        }
    }

    Ok(HubData {
        title: "少数派 -- 最新上架付费专栏".to_string(),
        description: Some("少数派最新上架付费专栏，仅作更新提醒，不含付费内容。".to_string()),
        link: Some("https://sspai.com/series".to_string()),
        image: None,
        language: None,
        items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_SSPAI_SERIES: Route = Route {
    meta: &META_SSPAI_SERIES,
    handler: handler_fn,
};
