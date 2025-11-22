use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset};
use scraper::{Html, Selector};
use serde_json::Value;

const ROOT_URL: &str = "https://podcasts.apple.com";

pub const META_APPLE_PODCAST: RouteMeta = RouteMeta {
    hub_id: "apple/podcast",
    path: "/apple/podcast/:id/:region?",
    categories: &["multimedia"],
    example: "/apple/podcast/id1559695855/cn",
    params: &[
        ParamMeta {
            name: "id",
            description: "播客 id，可以从 Apple Podcasts 分享链接中获得，例如 id1559695855。",
            default: None,
            options: &[],
        },
        ParamMeta {
            name: "region",
            description: "地区代码，例如 cn、us、jp，默认 cn。",
            default: Some("cn"),
            options: &[],
        },
        ParamMeta {
            name: "limit",
            description: "最大单集数量（默认 20）。",
            default: Some("20"),
            options: &[],
        },
    ],
    features: Features::basic(),
    radar: &[Radar {
        source: &[
            "podcasts.apple.com/:region/podcast/:showName/:id",
            "podcasts.apple.com/:region/podcast/:id",
        ],
        target: "/podcast/:id/:region?",
    }],
    name: "Apple Podcasts 节目",
    maintainers: &["captura"],
    url: "https://www.apple.com/apple-podcasts/",
    description: "Apple Podcasts 某个节目的最新单集列表（基于页面 JSON-LD，暂不提供音频直链）。",
    default_view: Some("audios"),
};

fn parse_date(s: &str) -> Option<DateTime<FixedOffset>> {
    util::parse_date(s)
}

async fn fetch_show_html(id: &str, region: &str) -> Result<String> {
    let url = format!("{}/{}/podcast/{}", ROOT_URL, region, id);
    util::get_html(&url).await
}

fn extract_ld_json(html: &str) -> Result<Value> {
    let doc = Html::parse_document(html);
    // 使用 id="schema:show" 的 JSON-LD
    let sel = Selector::parse(r#"script[id="schema:show"]"#)
        .map_err(|e| Error::Parse(format!("apple/podcast: selector error: {e}")))?;
    let script = doc
        .select(&sel)
        .next()
        .ok_or_else(|| Error::Parse("apple/podcast: schema:show script not found".to_string()))?;
    let json_str = script.text().collect::<String>();
    serde_json::from_str(&json_str)
        .map_err(|e| Error::Parse(format!("apple/podcast: invalid JSON-LD: {e}")))
}

fn build_items_from_ld(
    root: &Value,
    limit: usize,
) -> (String, Option<String>, Option<String>, Vec<HubItem>) {
    let title = root
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("Apple Podcasts")
        .to_string();
    let description = root
        .get("description")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let image = root
        .get("image")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // 作者从 publisher.name 或 author 字段尝试提取
    let series_author = root
        .get("publisher")
        .and_then(|p| p.get("name"))
        .and_then(|v| v.as_str())
        .or_else(|| root.get("author").and_then(|v| v.as_str()))
        .map(|s| s.to_string());

    let has_part = root
        .get("hasPart")
        .or_else(|| root.get("workExample"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut items = Vec::new();

    for ep in has_part.into_iter().take(limit) {
        let ep_title = ep
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if ep_title.is_empty() {
            continue;
        }

        let link = ep
            .get("url")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let desc = ep
            .get("description")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let date_raw = ep
            .get("datePublished")
            .or_else(|| ep.get("uploadDate"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let pub_date = parse_date(date_raw);

        // 归类：基于 genre 字段 + 通用标签
        let mut categories = Vec::new();
        if let Some(genres) = ep.get("genre").and_then(|v| v.as_array()) {
            for g in genres {
                if let Some(s) = g.as_str() {
                    let s = s.trim();
                    if !s.is_empty() && !categories.contains(&s.to_string()) {
                        categories.push(s.to_string());
                    }
                }
            }
        }
        if !categories.iter().any(|c| c.eq_ignore_ascii_case("podcast")) {
            categories.push("podcast".to_string());
        }
        if !categories.iter().any(|c| c.eq_ignore_ascii_case("apple")) {
            categories.push("apple".to_string());
        }

        items.push(HubItem {
            title: ep_title,
            description: desc,
            link,
            author: series_author.clone(),
            pub_date,
            categories,
        });
    }

    (title, description, image, items)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let id = ctx.param_str("id").unwrap_or("").trim().to_string();
    if id.is_empty() {
        return Err(captura_common::Error::Parse(
            "apple/podcast: id is required".to_string(),
        ));
    }
    let region = ctx.param_str("region").unwrap_or("cn");
    let limit = ctx.param_i64("limit").unwrap_or(20).max(1) as usize;

    let html = fetch_show_html(&id, region).await?;
    let ld = extract_ld_json(&html)?;

    let (title, description, image, items) = build_items_from_ld(&ld, limit);

    // JSON-LD 中通常包含节目的 URL，但如果没有，就退回到构造的页面 URL
    let link = ld
        .get("url")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("{}/{}/podcast/{}", ROOT_URL, region, id));

    Ok(HubData {
        title,
        description,
        link: Some(link),
        image,
        language: None,
        items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_APPLE_PODCAST: Route = Route {
    meta: &META_APPLE_PODCAST,
    handler: handler_fn,
};
