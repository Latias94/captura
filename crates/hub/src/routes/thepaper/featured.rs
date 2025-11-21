use crate::routes::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use crate::routes::util;
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset};
use scraper::{Html, Selector};
use serde_json::Value;

const ROOT_URL: &str = "https://m.thepaper.cn";

pub const META_THEPAPER_FEATURED: RouteMeta = RouteMeta {
    hub_id: "thepaper/featured",
    path: "/thepaper/featured",
    categories: &["new-media"],
    example: "/thepaper/featured",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["thepaper.cn/"],
        target: "/featured",
    }],
    name: "澎湃新闻首页头条",
    maintainers: &["captura"],
    url: "https://m.thepaper.cn",
    description:
        "澎湃新闻移动端首页头条列表，基于 __NEXT_DATA__ JSON 的简化实现，对齐 RSSHub /thepaper/featured 路由。",
    default_view: Some("articles"),
};

fn extract_items_from_next(json: &Value, limit: usize) -> Result<Vec<HubItem>> {
    let data = json
        .get("props")
        .and_then(|v| v.get("pageProps"))
        .and_then(|v| v.get("data"))
        .ok_or_else(|| Error::Parse("thepaper: missing pageProps.data".to_string()))?;

    let list = data
        .get("list")
        .and_then(|v| v.as_array())
        .ok_or_else(|| Error::Parse("thepaper: data.list is not array".to_string()))?;

    let mut items = Vec::new();

    for item in list.iter().take(limit) {
        let name = item
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let cont_id = item
            .get("contId")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if name.is_empty() || cont_id.is_empty() {
            continue;
        }

        let corner = item
            .get("cornerLabelDesc")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let path = if corner == "短剧" {
            "series"
        } else {
            "detail"
        };
        let link = format!("{}/{}/{}", ROOT_URL, path, cont_id);

        let pic = item
            .get("pic")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let pub_time_long = item
            .get("pubTimeLong")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let pub_date = util::parse_ms_timestamp(pub_time_long, 0);

        let node_name = item
            .get("nodeInfo")
            .and_then(|v| v.get("name"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let mut description = String::new();
        if let Some(pic_url) = &pic {
            description.push_str(&format!(
                "<p><img src=\"{src}\" alt=\"{alt}\"></p>",
                src = pic_url,
                alt = name
            ));
        }

        let categories = if let Some(node) = node_name {
            vec![node]
        } else {
            Vec::new()
        };

        items.push(HubItem {
            title: name,
            description: if description.is_empty() {
                None
            } else {
                Some(description)
            },
            link: Some(link),
            author: None,
            pub_date,
            categories,
        });
    }

    Ok(items)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let limit = ctx.param_i64("limit").unwrap_or(20).max(1) as usize;
    let html = util::get_html(ROOT_URL).await?;
    let doc = Html::parse_document(&html);

    let json: Value = util::extract_next_data(&html)?;

    let items = extract_items_from_next(&json, limit)?;

    let sel_title = Selector::parse("title").unwrap();
    let sel_desc = Selector::parse(r#"meta[name="description"]"#).unwrap();

    let title = doc
        .select(&sel_title)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .unwrap_or_else(|| "澎湃新闻 - 首页头条".to_string());
    let description = doc
        .select(&sel_desc)
        .next()
        .and_then(|el| el.value().attr("content"))
        .map(|s| s.to_string());

    Ok(HubData {
        title,
        description,
        link: Some(ROOT_URL.to_string()),
        image: None,
        language: Some("zh-CN".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_THEPAPER_FEATURED: Route = Route {
    meta: &META_THEPAPER_FEATURED,
    handler: handler_fn,
};
