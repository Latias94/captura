use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;
use chrono::{DateTime, FixedOffset, TimeZone};
use serde::Deserialize;

const API_LIST: &str = "https://cloud.tencent.com/developer/api/home/article-list";
const API_CLASSIFY: &str =
    "https://cloud.tencent.com/developer/api/column/get-classify-list-by-scene";

const PAGE: i64 = 1;
const PAGE_SIZE: i64 = 20;

pub const META_TENCENT_CLOUD_DEVELOPER_COLUMN: RouteMeta = RouteMeta {
    hub_id: "tencent/cloud/developer/column",
    path: "/tencent/cloud/developer/column/:categoryId?",
    categories: &["programming"],
    example: "/tencent/cloud/developer/column/1",
    params: &[ParamMeta {
        name: "categoryId",
        description: "专栏分类 ID，来源于页面 URL，默认 0（全部）。",
        default: Some("0"),
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["cloud.tencent.com/developer/column"],
        target: "/cloud/developer/column/:categoryId?",
    }],
    name: "腾讯云开发者社区专栏",
    maintainers: &["captura"],
    url: "https://cloud.tencent.com/developer/column",
    description: "腾讯云开发者社区各专栏文章列表，对标 RSSHub /tencent/cloud/developer/column/:categoryId 路由。",
    default_view: Some("articles"),
};

#[derive(Debug, Deserialize)]
struct TencentListResp {
    list: Vec<TencentArticle>,
}

#[derive(Debug, Deserialize)]
struct TencentArticle {
    articleId: i64,
    title: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    createTime: Option<i64>,
    #[serde(default)]
    author: Option<TencentAuthor>,
    #[serde(default)]
    tags: Vec<TencentTag>,
}

#[derive(Debug, Deserialize)]
struct TencentAuthor {
    #[serde(default)]
    nickname: String,
}

#[derive(Debug, Deserialize)]
struct TencentTag {
    #[serde(default)]
    tagName: String,
}

#[derive(Debug, Deserialize)]
struct TencentClassifyResp {
    list: Vec<TencentClassify>,
}

#[derive(Debug, Deserialize)]
struct TencentClassify {
    id: i64,
    name: String,
}

fn parse_timestamp(ts: Option<i64>) -> Option<DateTime<FixedOffset>> {
    let ts = ts?;
    // API 返回的是秒级时间戳。
    let offset = FixedOffset::east_opt(8 * 3600)?;
    Some(offset.timestamp_opt(ts, 0).single()?)
}

async fn fetch_classify_name(category_id: i64) -> Result<Option<String>> {
    if category_id == 0 {
        return Ok(None);
    }
    let client = client_basic(None, None)
        .map_err(|e| Error::Network(format!("tencent client error: {}", e)))?;
    let body = serde_json::json!({ "scene": 0 });

    let resp = client
        .post(API_CLASSIFY)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{} -> {}", API_CLASSIFY, e)))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "{} -> http status {}",
            API_CLASSIFY, status
        )));
    }
    let parsed: TencentClassifyResp = resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("tencent classify parse error: {}", e)))?;
    Ok(parsed
        .list
        .into_iter()
        .find(|c| c.id == category_id)
        .map(|c| c.name))
}

async fn fetch_list(category_id: i64, limit: usize) -> Result<Vec<TencentArticle>> {
    let client = client_basic(None, None)
        .map_err(|e| Error::Network(format!("tencent client error: {}", e)))?;
    let body = serde_json::json!({
        "classifyId": category_id,
        "page": PAGE,
        "pagesize": PAGE_SIZE.min(limit as i64),
        "type": ""
    });

    let resp = client
        .post(API_LIST)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{} -> {}", API_LIST, e)))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "{} -> http status {}",
            API_LIST, status
        )));
    }
    let parsed: TencentListResp = resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("tencent list parse error: {}", e)))?;
    Ok(parsed.list)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let category_str = ctx.param_str("categoryId").unwrap_or("0");
    let category_id = category_str.parse::<i64>().unwrap_or(0);
    let limit = ctx.param_i64("limit").unwrap_or(20).max(1) as usize;

    let classify_name = fetch_classify_name(category_id).await.unwrap_or(None);
    let articles = fetch_list(category_id, limit).await?;

    let mut items = Vec::new();
    for a in articles.into_iter().take(limit) {
        let link = format!(
            "https://cloud.tencent.com/developer/article/{}",
            a.articleId
        );
        let pub_date = parse_timestamp(a.createTime);
        let author = a
            .author
            .as_ref()
            .map(|au| au.nickname.clone())
            .filter(|s| !s.is_empty());
        let categories = a.tags.iter().map(|t| t.tagName.clone()).collect::<Vec<_>>();

        items.push(HubItem {
            title: a.title,
            description: if a.summary.is_empty() {
                None
            } else {
                Some(a.summary.clone())
            },
            link: Some(link),
            author,
            pub_date,
            categories,
        });
    }

    let title = match classify_name {
        Some(ref name) => format!("腾讯云开发者社区专栏 - {}", name),
        None => "腾讯云开发者社区专栏".to_string(),
    };
    let desc = title.clone();

    Ok(HubData {
        title,
        description: Some(desc),
        link: Some("https://cloud.tencent.com/developer/column".to_string()),
        image: None,
        language: Some("zh-CN".to_string()),
        items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_TENCENT_CLOUD_DEVELOPER_COLUMN: Route = Route {
    meta: &META_TENCENT_CLOUD_DEVELOPER_COLUMN,
    handler: handler_fn,
};
