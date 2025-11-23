use crate::routes::bangumi_tv::{API_ROOT, WEB_ROOT, local_name};
use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use serde_json::Value;

pub const META_BANGUMI_SUBJECT_BLOGS: RouteMeta = RouteMeta {
    hub_id: "bangumi.tv/subject_blogs",
    path: "/bangumi.tv/subject/:id/blogs/:show_original_name?",
    categories: &["anime"],
    example: "/bangumi.tv/subject/328609/blogs/true",
    params: &[
        ParamMeta {
            name: "id",
            description: "Bangumi subject id, e.g. 328609.",
            default: None,
            options: &[],
        },
        ParamMeta {
            name: "show_original_name",
            description: "Whether to show original title (true/false, 1/0), default false (show localized name if available).",
            default: Some("false"),
            options: &[
                ("false", "Use localized title when possible"),
                ("true", "Always use original title"),
            ],
        },
    ],
    features: Features::basic(),
    radar: &[Radar {
        source: &["bgm.tv/subject/:id", "bangumi.tv/subject/:id"],
        target: "/subject/:id/blogs",
    }],
    name: "Bangumi 条目长评",
    maintainers: &["captura"],
    url: "https://bangumi.tv",
    description: "Bangumi.tv subject long reviews via official API, aligned with RSSHub /bangumi.tv/subject/:id/blogs route.",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let id = ctx
        .param_str("id")
        .ok_or_else(|| Error::Config("bangumi.tv/subject_blogs: missing subject id".to_string()))?;
    let show_original = ctx
        .param_str("show_original_name")
        .map(|v| matches!(v, "1" | "true" | "True" | "TRUE"))
        .unwrap_or(false);
    let limit = ctx.param_i64("limit").unwrap_or(50).max(1) as usize;

    let api_url = format!("{}/subject/{}?responseGroup=large", API_ROOT, id);
    let subject: Value = util::get_json(&api_url)
        .await
        .map_err(|e| Error::Network(format!("bangumi.tv subject api error: {}", e)))?;

    let sid = subject
        .get("id")
        .and_then(|v| v.as_i64())
        .unwrap_or_else(|| id.parse().unwrap_or(0));
    let name = subject.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let name_cn = subject
        .get("name_cn")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let summary = subject
        .get("summary")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let subject_title = local_name(name, name_cn, show_original);
    let subject_link = format!("{}/subject/{}/reviews", WEB_ROOT, sid);

    let mut items = Vec::new();

    if let Some(blogs) = subject.get("blog").and_then(|v| v.as_array()) {
        for blog in blogs.iter().take(limit) {
            let blog_title = blog
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let summary_text = blog
                .get("summary")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let user_nickname = blog
                .get("user")
                .and_then(|u| u.get("nickname"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let timestamp = blog.get("timestamp").and_then(|v| v.as_i64()).unwrap_or(0);
            let mut link = blog
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if link.starts_with("http:") {
                link = link.replacen("http:", "https:", 1);
            }

            let title = if user_nickname.trim().is_empty() {
                blog_title.clone()
            } else {
                format!("{}：{}", user_nickname.trim(), blog_title)
            };

            let description = if summary_text.trim().is_empty() {
                None
            } else {
                Some(summary_text)
            };

            let pub_date = util::parse_unix_timestamp(timestamp, 0);

            items.push(HubItem {
                title,
                description,
                link: Some(link),
                author: if user_nickname.trim().is_empty() {
                    None
                } else {
                    Some(user_nickname.trim().to_string())
                },
                pub_date,
                categories: vec![
                    "Bangumi".to_string(),
                    "Anime".to_string(),
                    "Blogs".to_string(),
                ],
            });
        }
    }

    Ok(HubData {
        title: format!("{}的 Bangumi 评论", subject_title),
        description: if summary.trim().is_empty() {
            None
        } else {
            Some(summary)
        },
        link: Some(subject_link),
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
pub const ROUTE_BANGUMI_SUBJECT_BLOGS: Route = Route {
    meta: &META_BANGUMI_SUBJECT_BLOGS,
    handler: handler_fn,
};
