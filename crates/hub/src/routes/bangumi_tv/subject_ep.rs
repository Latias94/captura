use crate::routes::bangumi_tv::{local_name, API_ROOT, WEB_ROOT};
use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset};
use serde_json::Value;

fn parse_airdate(s: &str) -> Option<DateTime<FixedOffset>> {
    crate::routes::util::parse_date(s)
}

pub const META_BANGUMI_SUBJECT_EP: RouteMeta = RouteMeta {
    hub_id: "bangumi.tv/subject_ep",
    path: "/bangumi.tv/subject/:id/ep/:show_original_name?",
    categories: &["anime"],
    example: "/bangumi.tv/subject/328609/ep/true",
    params: &[
        ParamMeta {
            name: "id",
            description: "Bangumi subject id, e.g. 328609.",
            default: None,
            options: &[],
        },
        ParamMeta {
            name: "show_original_name",
            description:
                "Whether to show original title (true/false, 1/0), default false (show localized name if available).",
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
        target: "/subject/:id/ep",
    }],
    name: "Bangumi 条目剧集列表",
    maintainers: &["captura"],
    url: "https://bangumi.tv",
    description:
        "Bangumi.tv subject episode feed via official API, aligned with RSSHub /bangumi.tv/subject/:id/ep route.",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let id = ctx
        .param_str("id")
        .ok_or_else(|| Error::Config("bangumi.tv/subject_ep: missing subject id".to_string()))?;
    let show_original = ctx
        .param_str("show_original_name")
        .map(|v| matches!(v, "1" | "true" | "True" | "TRUE"))
        .unwrap_or(false);
    let limit = ctx.param_i64("limit").unwrap_or(100).max(1) as usize;

    let api_url = format!("{}/subject/{}?responseGroup=large", API_ROOT, id);
    let subject: Value = util::get_json(&api_url)
        .await
        .map_err(|e| Error::Network(format!("bangumi.tv subject api error: {}", e)))?;

    let sid = subject
        .get("id")
        .and_then(|v| v.as_i64())
        .unwrap_or_else(|| id.parse().unwrap_or(0));
    let name = subject
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let name_cn = subject
        .get("name_cn")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let summary = subject
        .get("summary")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let subject_title = local_name(&name, &name_cn, show_original);
    let subject_link = format!("{}/subject/{}", WEB_ROOT, sid);

    let mut items = Vec::new();

    if let Some(eps) = subject.get("eps").and_then(|v| v.as_array()) {
        for ep in eps
            .iter()
            .filter(|e| e.get("status").and_then(|s| s.as_str()).unwrap_or("") == "Air")
            .take(limit)
        {
            let ep_name = ep
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let ep_name_cn = ep
                .get("name_cn")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let ep_title = local_name(&ep_name, &ep_name_cn, show_original);
            if ep_title.is_empty() {
                continue;
            }
            let sort = ep.get("sort").and_then(|v| v.as_i64()).unwrap_or(0);
            let title = format!("ep.{} {}", sort, ep_title);

            let mut description = String::new();
            if !summary.trim().is_empty() {
                description.push_str(&format!(
                    "<p><strong>{}</strong></p><p>{}</p>",
                    subject_title, summary
                ));
            }
            let airdate = ep
                .get("airdate")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if !airdate.is_empty() {
                description.push_str(&format!("<p>Air date: {}</p>", airdate));
            }

            let mut link = ep
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if link.starts_with("http:") {
                link = link.replacen("http:", "https:", 1);
            } else if link.starts_with("/ep/") {
                link = format!("{}{}", WEB_ROOT, link);
            }

            let pub_date = parse_airdate(ep.get("airdate").and_then(|v| v.as_str()).unwrap_or(""));

            items.push(HubItem {
                title,
                description: if description.is_empty() {
                    None
                } else {
                    Some(description)
                },
                link: Some(link),
                author: None,
                pub_date,
                categories: vec!["Bangumi".to_string(), "Anime".to_string()],
            });
        }
    }

    Ok(HubData {
        title: subject_title,
        description: if summary.trim().is_empty() {
            None
        } else {
            Some(summary)
        },
        link: Some(subject_link),
        image: None,
        language: Some("ja-JP".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_BANGUMI_SUBJECT_EP: Route = Route {
    meta: &META_BANGUMI_SUBJECT_EP,
    handler: handler_fn,
};
