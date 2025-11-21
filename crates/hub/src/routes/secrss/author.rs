use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;
use chrono::{DateTime, FixedOffset};
use scraper::{Html, Selector};
use serde::Deserialize;

const ROOT_URL: &str = "https://www.secrss.com";
const API_URL: &str = "https://www.secrss.com/api/articles/group";

pub const META_SECRSS_AUTHOR: RouteMeta = RouteMeta {
    hub_id: "secrss/author",
    path: "/secrss/author/:author",
    categories: &["security"],
    example: "/secrss/author/网络安全威胁和漏洞信息共享平台",
    params: &[ParamMeta {
        name: "author",
        description:
            "Author name, as displayed on Secrss (e.g. 网络安全威胁和漏洞信息共享平台).",
        default: None,
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["www.secrss.com/articles"],
        target: "/author/:author",
    }],
    name: "安全内参 - 作者文章",
    maintainers: &["captura"],
    url: "https://www.secrss.com",
    description:
        "Secrss articles by a specific author, using the official JSON API and article pages, aligned with RSSHub /secrss/author/:author.",
    default_view: Some("articles"),
};

#[derive(Debug, Deserialize)]
struct SecrssResponse {
    #[serde(default)]
    code: String,
    #[serde(default)]
    msg: String,
    data: SecrssData,
}

#[derive(Debug, Deserialize)]
struct SecrssData {
    #[serde(default)]
    list: Vec<SecrssItem>,
}

#[derive(Debug, Deserialize)]
struct SecrssItem {
    #[serde(default)]
    title: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    img: String,
    #[serde(default)]
    date: String,
    #[serde(default)]
    tag: String,
    #[serde(default)]
    taglink: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    original_timestamp: i64,
}

fn parse_pub_date(ts: i64) -> Option<DateTime<FixedOffset>> {
    util::parse_unix_timestamp(ts, 8)
}

async fn fetch_author_items(author: &str, limit: usize) -> Result<Vec<SecrssItem>> {
    let client = client_basic(None, None)
        .map_err(|e| Error::Network(format!("secrss client error: {}", e)))?;

    let resp = client
        .get(API_URL)
        .query(&[("author", author)])
        .send()
        .await
        .map_err(|e| Error::Network(format!("{API_URL} -> {e}")))?;

    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!("{API_URL} -> http status {status}")));
    }

    let json: SecrssResponse = resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("secrss: json parse error: {e}")))?;

    if json.code != "10000" {
        return Err(Error::Network(format!(
            "secrss: api returned code {} ({})",
            json.code, json.msg
        )));
    }

    Ok(json.data.list.into_iter().take(limit).collect())
}

fn extract_article_body(html: &str) -> Option<String> {
    let doc = Html::parse_document(html);
    let sel = Selector::parse(".article-body").ok()?;
    let body = doc.select(&sel).next()?;
    let html = body.html();
    if html.trim().is_empty() {
        None
    } else {
        Some(html)
    }
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let author = ctx.param_str("author").unwrap_or("").trim().to_string();
    if author.is_empty() {
        return Err(captura_common::Error::Parse(
            "author is required".to_string(),
        ));
    }
    let limit = ctx.param_i64("limit").unwrap_or(20).max(1) as usize;

    let items_raw = fetch_author_items(&author, limit).await?;
    let mut items = Vec::new();

    for item in items_raw.into_iter().take(limit) {
        if item.title.trim().is_empty() || item.url.trim().is_empty() {
            continue;
        }

        let link = format!("{ROOT_URL}{}", item.url);

        let description = match util::get_html(&link).await {
            Ok(html) => extract_article_body(&html).or_else(|| {
                if item.summary.trim().is_empty() {
                    None
                } else {
                    Some(format!("<p>{}</p>", item.summary.trim()))
                }
            }),
            Err(_) => {
                if item.summary.trim().is_empty() {
                    None
                } else {
                    Some(format!("<p>{}</p>", item.summary.trim()))
                }
            }
        };

        let pub_date = parse_pub_date(item.original_timestamp);

        let mut categories = Vec::new();
        if !item.tag.trim().is_empty() {
            categories.push(item.tag.trim().to_string());
        }

        items.push(HubItem {
            title: item.title.trim().to_string(),
            description,
            link: Some(link),
            author: Some(author.clone()),
            pub_date,
            categories,
        });
    }

    Ok(HubData {
        title: format!("安全内参 - {}", author),
        description: Some(format!("安全内参作者「{}」的文章列表。", author)),
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
pub const ROUTE_SECRSS_AUTHOR: Route = Route {
    meta: &META_SECRSS_AUTHOR,
    handler: handler_fn,
};
