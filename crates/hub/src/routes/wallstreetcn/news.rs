use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset};
use serde_json::Value;

const ROOT_URL: &str = "https://wallstreetcn.com";
const API_ROOT: &str = "https://api-one.wallstcn.com";

pub const META_WALLSTREETCN_NEWS: RouteMeta = RouteMeta {
    hub_id: "wallstreetcn/news",
    path: "/wallstreetcn/news/:category?",
    categories: &["finance"],
    example: "/wallstreetcn/news",
    params: &[
        ParamMeta {
            name: "category",
            description:
                "News category, e.g. global, shares, bonds, commodities, forex, enterprise, asset-manage, tmt, estate, car, medicine.",
            default: Some("global"),
            options: &[
                ("global", "Latest"),
                ("shares", "Shares"),
                ("bonds", "Bonds"),
                ("commodities", "Commodities"),
                ("forex", "Forex"),
                ("enterprise", "Enterprise"),
                ("asset-manage", "Asset management"),
                ("tmt", "Technology"),
                ("estate", "Real estate"),
                ("car", "Automotive"),
                ("medicine", "Medicine"),
            ],
        },
    ],
    features: Features::basic(),
    radar: &[Radar {
        source: &["wallstreetcn.com/news/:category", "wallstreetcn.com"],
        target: "/news/:category",
    }],
    name: "华尔街见闻资讯",
    maintainers: &["captura"],
    url: "https://wallstreetcn.com",
    description:
        "WallstreetCN news stream via official `api-one.wallstcn.com` JSON API, aligned with RSSHub /wallstreetcn/news/:category route.",
    default_view: Some("articles"),
};

fn category_title(category: &str) -> &'static str {
    match category {
        "shares" => "股市",
        "bonds" => "债市",
        "commodities" => "商品",
        "forex" => "外汇",
        "enterprise" => "公司",
        "asset-manage" => "资管",
        "tmt" => "科技",
        "estate" => "地产",
        "car" => "汽车",
        "medicine" => "医药",
        _ => "最新",
    }
}

fn parse_display_time(ts: i64) -> Option<DateTime<FixedOffset>> {
    if ts <= 0 {
        return None;
    }
    util::parse_ms_timestamp(ts * 1000, 8)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let category = ctx.param_str("category").unwrap_or("global");
    let limit = ctx.param_i64("limit").unwrap_or(25).max(1) as usize;

    let current_url = format!("{}/news/{}", ROOT_URL, category);
    let api_url = format!(
        "{}/apiv1/content/information-flow?channel={}-channel&accept=article&limit={}",
        API_ROOT, category, limit
    );

    let resp: Value = util::get_json(&api_url)
        .await
        .map_err(|e| Error::Network(format!("wallstreetcn list api error: {}", e)))?;

    let items_value = resp
        .get("data")
        .and_then(|d| d.get("items"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut items = Vec::new();

    for item in items_value
        .into_iter()
        .filter(|v| {
            v.get("resource_type")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                != "ad"
        })
        .take(limit)
    {
        let resource_type = item
            .get("resource_type")
            .and_then(|t| t.as_str())
            .unwrap_or("");
        let resource = match item.get("resource") {
            Some(r) => r,
            None => continue,
        };

        let id_str = resource
            .get("id")
            .and_then(|v| {
                v.as_str()
                    .map(|s| s.to_string())
                    .or_else(|| v.as_i64().map(|n| n.to_string()))
            })
            .unwrap_or_default();
        if id_str.is_empty() {
            continue;
        }

        let uri = resource.get("uri").and_then(|v| v.as_str()).unwrap_or("");
        if uri.is_empty() {
            continue;
        }
        let link = util::absolutize(ROOT_URL, uri);

        let display_time = resource
            .get("display_time")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let pub_date = parse_display_time(display_time);

        let detail_api = if resource_type == "live" {
            format!("{}/apiv1/content/lives/{}", API_ROOT, id_str)
        } else {
            format!("{}/apiv1/content/articles/{}?extract=0", API_ROOT, id_str)
        };

        let detail: Value = match util::get_json(&detail_api).await {
            Ok(v) => v,
            Err(_) => continue,
        };

        let code = detail.get("code").and_then(|v| v.as_i64()).unwrap_or(0);
        if code != 20000 {
            continue;
        }

        let data = match detail.get("data") {
            Some(d) => d,
            None => continue,
        };

        let mut title = data
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if title.is_empty() {
            title = data
                .get("content_text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
        }
        if title.is_empty() {
            continue;
        }

        let author = data
            .get("source_name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| {
                data.get("author")
                    .and_then(|a| a.get("display_name"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            });

        let mut description = String::new();
        if let Some(content) = data.get("content").and_then(|v| v.as_str()) {
            description.push_str(content);
        }
        if let Some(more) = data.get("content_more").and_then(|v| v.as_str()) {
            description.push_str(more);
        }
        if description.is_empty() {
            description.push_str(&link);
        }

        let mut categories = Vec::new();
        if let Some(tags) = data.get("asset_tags").and_then(|v| v.as_array()) {
            for t in tags {
                if let Some(name) = t.get("name").and_then(|v| v.as_str()) {
                    categories.push(name.to_string());
                }
            }
        }

        items.push(HubItem {
            title,
            description: Some(description),
            link: Some(link),
            author,
            pub_date,
            categories,
        });
    }

    let title = format!("华尔街见闻 - 资讯 - {}", category_title(category));

    Ok(HubData {
        title,
        description: Some("华尔街见闻资讯流，基于官方公开 API 抓取。".to_string()),
        link: Some(current_url),
        image: Some("https://static.wscn.net/wscn/_static/favicon.png".to_string()),
        language: Some("zh-CN".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_WALLSTREETCN_NEWS: Route = Route {
    meta: &META_WALLSTREETCN_NEWS,
    handler: handler_fn,
};
