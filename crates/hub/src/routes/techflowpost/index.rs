use crate::routes::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use captura_net::{client_basic, client_builder};
use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeZone};
use serde::Deserialize;

const ROOT_URL: &str = "https://www.techflowpost.com";
const API_URL: &str = "https://www.techflowpost.com/ashx/index.ashx";

pub const META_TECHFLOWPOST_INDEX: RouteMeta = RouteMeta {
    hub_id: "techflowpost",
    path: "/techflowpost",
    categories: &["finance"],
    example: "/techflowpost",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["www.techflowpost.com/"],
        target: "/",
    }],
    name: "深潮 TechFlow 首页",
    maintainers: &["captura"],
    url: "https://www.techflowpost.com/",
    description: "深潮 TechFlow homepage articles list via official JSON API, aligned with RSSHub /techflowpost route.",
    default_view: Some("articles"),
};

#[derive(Debug, Deserialize)]
struct TechflowResponse {
    #[serde(default)]
    success: String,
    #[serde(default)]
    content: Vec<TechflowItem>,
}

#[derive(Debug, Deserialize)]
struct TechflowItem {
    #[serde(default)]
    narticle_id: String,
    #[serde(default)]
    stitle: String,
    #[serde(default)]
    sabstract: String,
    #[serde(default)]
    scata_name: String,
    #[serde(default)]
    sauthor_name: String,
    #[serde(default)]
    dcreate_time: String,
    #[serde(default)]
    dmodi_time: String,
    #[serde(default)]
    scontent: String,
}

fn parse_datetime(s: &str) -> Option<DateTime<FixedOffset>> {
    // Example: 2025-11-21 12:39:44
    let naive = NaiveDateTime::parse_from_str(s.trim(), "%Y-%m-%d %H:%M:%S").ok()?;
    let offset = FixedOffset::east_opt(8 * 3600)?;
    Some(offset.from_utc_datetime(&naive))
}

async fn fetch_articles(limit: usize) -> Result<Vec<TechflowItem>> {
    let client = client_builder(None, None)?
        .gzip(false)
        .deflate(false)
        .brotli(false)
        .build()
        .map_err(|e| Error::Network(format!("techflowpost client error: {}", e)))?;

    let resp = client
        .post(API_URL)
        .header("Accept-Encoding", "identity")
        .form(&[("pageindex", "1"), ("pagesize", &limit.to_string())])
        .send()
        .await
        .map_err(|e| Error::Network(format!("{API_URL} -> {e}")))?;

    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!("{API_URL} -> http status {status}")));
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;
    let body = String::from_utf8_lossy(&bytes);

    let parsed: TechflowResponse = serde_json::from_str(&body)
        .map_err(|e| Error::Parse(format!("techflowpost: json parse error: {e}")))?;

    if parsed.success != "Y" {
        return Err(Error::Network(format!(
            "techflowpost: api returned success = {}",
            parsed.success
        )));
    }

    Ok(parsed.content)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let limit = ctx.param_i64("limit").unwrap_or(50).max(1) as usize;
    let articles = fetch_articles(limit).await?;

    let mut items = Vec::new();

    for item in articles.into_iter().take(limit) {
        let id = item.narticle_id.trim();
        if id.is_empty() || item.stitle.trim().is_empty() {
            continue;
        }

        let link = format!("{ROOT_URL}/article/detail_{}.html", id);

        let description = if !item.scontent.trim().is_empty() {
            Some(item.scontent.clone())
        } else if !item.sabstract.trim().is_empty() {
            Some(format!("<p>{}</p>", item.sabstract.trim()))
        } else {
            None
        };

        let author = if item.sauthor_name.trim().is_empty() {
            None
        } else {
            Some(item.sauthor_name.trim().to_string())
        };

        let pub_date =
            parse_datetime(&item.dcreate_time).or_else(|| parse_datetime(&item.dmodi_time));

        let mut categories = Vec::new();
        if !item.scata_name.trim().is_empty() {
            categories.push(item.scata_name.trim().to_string());
        }

        items.push(HubItem {
            title: item.stitle.trim().to_string(),
            description,
            link: Some(link),
            author,
            pub_date,
            categories,
        });
    }

    Ok(HubData {
        title: "深潮 TechFlow".to_string(),
        description: Some("深潮 TechFlow homepage articles list.".to_string()),
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
pub const ROUTE_TECHFLOWPOST_INDEX: Route = Route {
    meta: &META_TECHFLOWPOST_INDEX,
    handler: handler_fn,
};
