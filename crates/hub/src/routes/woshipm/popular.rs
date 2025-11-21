use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset, TimeZone};
use scraper::{Html, Selector};

const BASE_URL: &str = "https://www.woshipm.com";

pub const META_WOSHIPM_POPULAR: RouteMeta = RouteMeta {
    hub_id: "woshipm/popular",
    path: "/woshipm/popular/:range?",
    categories: &["new-media"],
    example: "/woshipm/popular",
    params: &[ParamMeta {
        name: "range",
        description: "Time range: daily / weekly / monthly, default daily.",
        default: Some("daily"),
        options: &[
            ("daily", "Daily"),
            ("weekly", "Weekly"),
            ("monthly", "Monthly"),
        ],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["woshipm.com/"],
        target: "/popular",
    }],
    name: "人人都是产品经理 · 热门文章",
    maintainers: &["captura"],
    url: "https://www.woshipm.com",
    description: "Popular articles from woshipm.com, aligned with RSSHub /woshipm/popular.",
    default_view: Some("articles"),
};

#[derive(Debug, serde::Deserialize)]
struct WoshipmResponse {
    #[serde(default)]
    RESULT: Vec<WoshipmResult>,
}

#[derive(Debug, serde::Deserialize)]
struct WoshipmResult {
    data: WoshipmArticle,
}

#[derive(Debug, serde::Deserialize)]
struct WoshipmArticle {
    #[serde(default)]
    articleAuthor: String,
    #[serde(default)]
    articleSummary: String,
    #[serde(default)]
    articleTitle: String,
    #[serde(default)]
    id: i64,
    #[serde(default)]
    imageUrl: String,
    #[serde(default)]
    publishTime: i64,
    #[serde(default)]
    tag: String,
    #[serde(default)]
    r#type: String,
}

fn parse_publish_time(ms: i64) -> Option<DateTime<FixedOffset>> {
    if ms <= 0 {
        return None;
    }
    let secs = ms / 1000;
    let nsecs = ((ms % 1000) * 1_000_000).max(0);
    let offset = FixedOffset::east_opt(0)?;
    let naive = chrono::NaiveDateTime::from_timestamp_opt(secs, nsecs as u32)?;
    Some(offset.from_utc_datetime(&naive))
}

fn normalize_range(range: &str) -> &str {
    match range {
        "weekly" | "monthly" => range,
        _ => "daily",
    }
}

fn build_api_url(range: &str) -> String {
    let r = normalize_range(range);
    format!("{}/api2/app/article/popular/{}", BASE_URL, r)
}

fn build_article_link(article: &WoshipmArticle) -> String {
    let t = if article.r#type.is_empty() {
        "ai"
    } else {
        article.r#type.as_str()
    };
    format!("{}/{}/{}.html", BASE_URL, t, article.id)
}

fn extract_content(html: &str) -> Option<String> {
    let doc = Html::parse_document(html);
    let sel = Selector::parse(".article--content").ok()?;
    let content = doc.select(&sel).next()?;
    let mut html = util::element_html(&content);
    if html.trim().is_empty() {
        None
    } else {
        // 简单移除“支持作者”区域，避免干扰正文阅读
        if let Some(idx) = html.find("support-author") {
            html.truncate(idx);
        }
        Some(html)
    }
}

async fn fetch_list(range: &str) -> Result<Vec<WoshipmArticle>> {
    let url = build_api_url(range);
    let resp: WoshipmResponse = util::get_json(&url).await?;
    Ok(resp.RESULT.into_iter().map(|r| r.data).collect())
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let range = ctx.param_str("range").unwrap_or("daily");
    let limit = ctx.param_i64("limit").unwrap_or(30).max(1) as usize;
    let list = fetch_list(&range).await?;

    let mut items = Vec::new();
    for article in list.into_iter().take(limit) {
        let link = build_article_link(&article);
        let mut description = None;

        if let Ok(html) = util::get_html(&link).await {
            description = extract_content(&html);
        }

        if description.is_none() && !article.imageUrl.is_empty() {
            let img = &article.imageUrl;
            description = Some(format!(
                "<p><img src=\"{src}\" alt=\"{alt}\"></p>",
                src = img,
                alt = article.articleTitle
            ));
        }

        let categories = if article.tag.trim().is_empty() {
            Vec::new()
        } else {
            article
                .tag
                .split_whitespace()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        };

        items.push(HubItem {
            title: article.articleTitle.clone(),
            description,
            link: Some(link),
            author: if article.articleAuthor.trim().is_empty() {
                None
            } else {
                Some(article.articleAuthor.trim().to_string())
            },
            pub_date: parse_publish_time(article.publishTime),
            categories,
        });
    }

    Ok(HubData {
        title: format!("热门文章 - {}", normalize_range(&range)),
        description: Some("Popular articles from 人人都是产品经理 (woshipm.com).".to_string()),
        link: Some(BASE_URL.to_string()),
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
pub const ROUTE_WOSHIPM_POPULAR: Route = Route {
    meta: &META_WOSHIPM_POPULAR,
    handler: handler_fn,
};
