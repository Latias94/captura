use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;
use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeZone};
use serde::Deserialize;

const API_TOPIC_INFO: &str = "https://www.infoq.cn/public/v1/topic/getInfo";
const API_ARTICLE_LIST: &str = "https://www.infoq.cn/public/v1/article/getList";
const API_DETAIL: &str = "https://www.infoq.cn/public/v1/article/getDetail";

pub const META_INFOQ_TOPIC: RouteMeta = RouteMeta {
    hub_id: "infoq/topic",
    path: "/infoq/topic/:id",
    categories: &["new-media"],
    example: "/infoq/topic/1",
    params: &[ParamMeta {
        name: "id",
        description:
            "话题 id，可在 InfoQ 全部话题页面 URL 中找到，例如 https://www.infoq.cn/topic/1。",
        default: Some("1"),
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["infoq.cn/topic/:id"],
        target: "/topic/:id",
    }],
    name: "InfoQ 话题",
    maintainers: &["captura"],
    url: "https://www.infoq.cn/topics",
    description: "InfoQ 话题文章列表，对标 RSSHub /infoq/topic/:id 路由。",
    default_view: Some("articles"),
};

#[derive(Debug, Deserialize)]
struct InfoqTopicInfoResp {
    code: i32,
    data: InfoqTopicInfo,
}

#[derive(Debug, Deserialize)]
struct InfoqTopicInfo {
    id: i64,
    name: String,
    #[serde(default)]
    desc: Option<String>,
    #[serde(default)]
    cover: Option<String>,
}

#[derive(Debug, Deserialize)]
struct InfoqListResp {
    code: i32,
    data: Vec<InfoqListItem>,
}

#[derive(Debug, Deserialize)]
struct InfoqListItem {
    uuid: String,
    #[serde(default)]
    publish_time: Option<i64>,
    #[serde(default)]
    article_title: String,
    #[serde(default)]
    article_summary: String,
    #[serde(default)]
    content_short: String,
    #[serde(default)]
    no_author: Option<String>,
    #[serde(default)]
    topic: Vec<InfoqName>,
    #[serde(default)]
    label: Vec<InfoqName>,
}

#[derive(Debug, Deserialize)]
struct InfoqDetailResp {
    code: i32,
    data: InfoqDetailData,
}

#[derive(Debug, Deserialize)]
struct InfoqDetailData {
    uuid: String,
    article_title: String,
    #[serde(default)]
    publish_time: Option<i64>,
    #[serde(default)]
    author: Option<Vec<InfoqAuthor>>,
    #[serde(default)]
    no_author: Option<String>,
    #[serde(default)]
    topic: Vec<InfoqName>,
    #[serde(default)]
    label: Vec<InfoqName>,
    #[serde(default)]
    content_url: Option<String>,
    #[serde(default)]
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct InfoqAuthor {
    nickname: String,
}

#[derive(Debug, Deserialize)]
struct InfoqName {
    name: String,
}

fn parse_publish_time(ms: Option<i64>) -> Option<DateTime<FixedOffset>> {
    let ms = ms?;
    let sec = ms / 1000;
    let nsec = ((ms % 1000) * 1_000_000) as u32;
    let naive = NaiveDateTime::from_timestamp_opt(sec, nsec)?;
    let offset = FixedOffset::east_opt(8 * 3600)?;
    Some(offset.from_utc_datetime(&naive))
}

async fn fetch_topic_info(id_or_alias: &str, page_url: &str) -> Result<InfoqTopicInfo> {
    let client = client_basic(None, None)
        .map_err(|e| Error::Network(format!("infoq client error: {}", e)))?;

    let body = if id_or_alias.parse::<i64>().is_ok() {
        serde_json::json!({ "id": id_or_alias.parse::<i64>().unwrap() })
    } else {
        serde_json::json!({ "alias": id_or_alias })
    };

    let resp = client
        .post(API_TOPIC_INFO)
        .header("Referer", page_url)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{} -> {}", API_TOPIC_INFO, e)))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "{} -> http status {}",
            API_TOPIC_INFO, status
        )));
    }
    let parsed: InfoqTopicInfoResp = resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("infoq topic info parse error: {}", e)))?;
    Ok(parsed.data)
}

async fn fetch_list(topic_id: i64, limit: usize, page_url: &str) -> Result<Vec<InfoqListItem>> {
    let client = client_basic(None, None)
        .map_err(|e| Error::Network(format!("infoq client error: {}", e)))?;
    let body = serde_json::json!({
        "id": topic_id,
        "ptype": 0,
        "size": limit,
        "type": 0,
    });

    let resp = client
        .post(API_ARTICLE_LIST)
        .header("Referer", page_url)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{} -> {}", API_ARTICLE_LIST, e)))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "{} -> http status {}",
            API_ARTICLE_LIST, status
        )));
    }
    let parsed: InfoqListResp = resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("infoq list parse error: {}", e)))?;
    Ok(parsed.data)
}

async fn fetch_detail(uuid: &str, referer: &str) -> Result<InfoqDetailData> {
    let client = client_basic(None, None)
        .map_err(|e| Error::Network(format!("infoq client error: {}", e)))?;
    let body = serde_json::json!({ "uuid": uuid });

    let resp = client
        .post(API_DETAIL)
        .header("Referer", referer)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{} -> {}", API_DETAIL, e)))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "{} -> http status {}",
            API_DETAIL, status
        )));
    }
    let parsed: InfoqDetailResp = resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("infoq detail parse error: {}", e)))?;
    Ok(parsed.data)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let id = ctx.param_str("id").unwrap_or("1");
    let limit = ctx.param_i64("limit").unwrap_or(30).max(1) as usize;
    let page_url = format!("https://www.infoq.cn/topic/{}", id);

    let info = fetch_topic_info(id, &page_url).await?;
    let list = fetch_list(info.id, limit, &page_url).await?;

    let mut items = Vec::new();
    for item in list.into_iter().take(limit) {
        let article_url = format!("https://www.infoq.cn/article/{}", item.uuid);
        let detail_res = fetch_detail(&item.uuid, &page_url).await;

        let (title, author, pub_date, categories, description) = match detail_res {
            Ok(detail) => {
                let author = if detail
                    .author
                    .as_ref()
                    .map(|v| !v.is_empty())
                    .unwrap_or(false)
                {
                    detail
                        .author
                        .as_ref()
                        .map(|list| {
                            list.iter()
                                .map(|a| a.nickname.clone())
                                .collect::<Vec<_>>()
                                .join(",")
                        })
                        .filter(|s| !s.is_empty())
                        .or_else(|| detail.no_author.clone())
                } else {
                    detail.no_author.clone()
                };

                let mut categories = Vec::new();
                for t in &detail.topic {
                    categories.push(t.name.clone());
                }
                for l in &detail.label {
                    categories.push(l.name.clone());
                }

                let pub_date = parse_publish_time(detail.publish_time.or(item.publish_time));

                let mut description = None;
                if let Some(url) = detail.content_url.as_deref() {
                    if let Ok(html) = util::get_html(url).await {
                        if !html.trim().is_empty() {
                            description = Some(html);
                        }
                    }
                }
                if description.is_none() {
                    if let Some(content) = detail.content.as_deref() {
                        if let Some(html) = crate::routes::infoq::parse_rich_content(content) {
                            description = Some(html);
                        } else if !content.trim().is_empty() {
                            description = Some(content.to_string());
                        }
                    }
                }

                (
                    detail.article_title,
                    author,
                    pub_date,
                    categories,
                    description,
                )
            }
            Err(_) => {
                let title = if item.article_title.is_empty() {
                    item.uuid.clone()
                } else {
                    item.article_title.clone()
                };
                let author = item.no_author.clone();
                let mut categories = Vec::new();
                for t in &item.topic {
                    categories.push(t.name.clone());
                }
                for l in &item.label {
                    categories.push(l.name.clone());
                }
                let pub_date = parse_publish_time(item.publish_time);

                let mut description = None;
                if !item.content_short.trim().is_empty() {
                    if let Some(html) =
                        crate::routes::infoq::parse_rich_content(&item.content_short)
                    {
                        description = Some(html);
                    }
                }
                if description.is_none() && !item.article_summary.trim().is_empty() {
                    description = Some(item.article_summary.clone());
                }

                (title, author, pub_date, categories, description)
            }
        };

        items.push(HubItem {
            title,
            description,
            link: Some(article_url),
            author,
            pub_date,
            categories,
        });
    }

    Ok(HubData {
        title: format!("InfoQ 话题 - {}", info.name),
        description: info.desc.clone(),
        link: Some(page_url),
        image: info.cover.clone(),
        language: Some("zh-CN".to_string()),
        items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_INFOQ_TOPIC: Route = Route {
    meta: &META_INFOQ_TOPIC,
    handler: handler_fn,
};
