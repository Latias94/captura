use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeZone};

const ROOT_URL: &str = "https://readhub.cn";
const API_ROOT: &str = "https://api.readhub.cn";

pub const META_READHUB_INDEX: RouteMeta = RouteMeta {
    hub_id: "readhub",
    path: "/readhub",
    categories: &["new-media"],
    example: "/readhub",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["readhub.cn/"],
        target: "/",
    }],
    name: "Readhub 热门话题",
    maintainers: &["captura"],
    url: "https://readhub.cn",
    description:
        "Readhub 热门话题列表，基于公开 JSON 接口的简化实现，对齐 RSSHub /readhub 路由的默认分类。",
    default_view: Some("articles"),
};

#[derive(Debug, serde::Deserialize)]
struct ReadhubTopicList {
    data: ReadhubTopicData,
}

#[derive(Debug, serde::Deserialize)]
struct ReadhubTopicData {
    items: Vec<ReadhubTopic>,
}

#[derive(Debug, serde::Deserialize)]
struct ReadhubTopic {
    uid: String,
    title: String,
    summary: String,
    #[serde(default)]
    siteNameDisplay: String,
    #[serde(default)]
    publishDate: String,
    #[serde(default)]
    newsAggList: Vec<ReadhubNews>,
    #[serde(default)]
    entityList: Vec<ReadhubEntity>,
    #[serde(default)]
    tagList: Vec<ReadhubEntity>,
}

#[derive(Debug, serde::Deserialize)]
struct ReadhubNews {
    #[serde(default)]
    url: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    siteNameDisplay: String,
}

#[derive(Debug, serde::Deserialize)]
struct ReadhubEntity {
    #[serde(default)]
    name: String,
}

fn parse_iso_datetime(s: &str) -> Option<DateTime<FixedOffset>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt);
    }
    if let Ok(naive) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        if let Some(offset) = FixedOffset::east_opt(0) {
            return Some(offset.from_utc_datetime(&naive));
        }
    }
    None
}

fn build_topic_items(list: ReadhubTopicList, limit: usize) -> Vec<HubItem> {
    list.data
        .items
        .into_iter()
        .take(limit)
        .map(|t| {
            let mut html = String::new();
            if !t.summary.trim().is_empty() {
                html.push_str(&format!("<p>{}</p>", t.summary.trim()));
            }
            if !t.newsAggList.is_empty() {
                html.push_str("<ul>");
                for n in &t.newsAggList {
                    if n.url.is_empty() && n.title.is_empty() {
                        continue;
                    }
                    if !n.url.is_empty() {
                        html.push_str(&format!(
                            r#"<li><a href="{url}">{title}</a>{site}</li>"#,
                            url = n.url,
                            title = if n.title.is_empty() {
                                n.url.clone()
                            } else {
                                n.title.clone()
                            },
                            site = if n.siteNameDisplay.is_empty() {
                                "".to_string()
                            } else {
                                format!(" — {}", n.siteNameDisplay)
                            }
                        ));
                    } else {
                        html.push_str(&format!("<li>{}</li>", n.title));
                    }
                }
                html.push_str("</ul>");
            }

            let mut categories = Vec::new();
            for e in t.entityList.into_iter().chain(t.tagList.into_iter()) {
                if !e.name.trim().is_empty() {
                    categories.push(e.name.trim().to_string());
                }
            }

            HubItem {
                title: t.title.clone(),
                description: if html.is_empty() { None } else { Some(html) },
                link: Some(format!("{}/topic/{}", ROOT_URL, t.uid)),
                author: if t.siteNameDisplay.trim().is_empty() {
                    None
                } else {
                    Some(t.siteNameDisplay.trim().to_string())
                },
                pub_date: parse_iso_datetime(&t.publishDate),
                categories,
            }
        })
        .collect()
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let limit = ctx.param_i64("limit").unwrap_or(30).max(1) as usize;
    let api_url = format!("{}/topic/list?type=1&page=1&size={}", API_ROOT, limit);

    let list: ReadhubTopicList = util::get_json(&api_url).await?;
    let items = build_topic_items(list, limit);

    // 抓取首页用于标题和描述
    let html = util::get_html(ROOT_URL).await?;
    let doc = scraper::Html::parse_document(&html);
    let sel_title = scraper::Selector::parse("title").unwrap();
    let sel_desc = scraper::Selector::parse(r#"meta[name="description"]"#).unwrap();

    let title = doc
        .select(&sel_title)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .unwrap_or_else(|| "Readhub 热门话题".to_string());
    let description = doc
        .select(&sel_desc)
        .next()
        .and_then(|el| el.value().attr("content"))
        .map(|s| s.to_string());

    Ok(HubData {
        title,
        description,
        link: Some(ROOT_URL.to_string()),
        image: Some("https://readhub.cn/icons/icon-192x192.png".to_string()),
        language: Some("zh-CN".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_READHUB_INDEX: Route = Route {
    meta: &META_READHUB_INDEX,
    handler: handler_fn,
};
