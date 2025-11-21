use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use chrono::{Duration, Utc};
use scraper::{Html, Selector};
use serde_json::Value;
use urlencoding;

const BASE_URL: &str = "https://dev.to";

pub const META_DEV_TO_TOP: RouteMeta = RouteMeta {
    hub_id: "dev.to/top",
    path: "/dev.to/top/:period",
    categories: &["programming"],
    example: "/dev.to/top/week",
    params: &[ParamMeta {
        name: "period",
        description: "Period: week, month, year, or infinity (default: week).",
        default: Some("week"),
        options: &[
            ("week", "Top posts in the last week"),
            ("month", "Top posts in the last month"),
            ("year", "Top posts in the last year"),
            ("infinity", "Top posts in recent years"),
        ],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["dev.to/top/:period"],
        target: "/top/:period",
    }],
    name: "dev.to Top Posts",
    maintainers: &["captura"],
    url: "https://dev.to",
    description:
        "Top dev.to articles fetched via the public feed_content API, aligned with RSSHub /dev.to/top/:period route.",
    default_view: Some("articles"),
};

fn compute_published_gte(period: &str) -> String {
    let now = Utc::now();
    let cutoff = match period {
        "week" => now - Duration::days(7),
        "month" => now - Duration::days(30),
        "year" => now - Duration::days(365),
        "infinity" | _ => now - Duration::days(365 * 5),
    };
    cutoff.to_rfc3339()
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let period = ctx.param_str("period").unwrap_or("week");
    let limit = ctx.param_i64("limit").unwrap_or(15).max(1).min(50) as usize;

    let cutoff = compute_published_gte(period);
    let encoded = urlencoding::encode(&cutoff);
    let api_url = format!(
        "https://dev.to/search/feed_content?per_page={}&sort_by=public_reactions_count&sort_direction=desc&approved=&class_name=Article&published_at%5Bgte%5D={}",
        limit, encoded
    );

    let resp: Value = util::get_json(&api_url)
        .await
        .map_err(|e| Error::Network(format!("dev.to feed_content api error: {}", e)))?;

    let results = resp
        .get("result")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut items = Vec::new();

    for item in results.into_iter().take(limit) {
        let path = item
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if path.is_empty() {
            continue;
        }
        let article_url = format!("{}{}", BASE_URL, path);

        let title = item
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if title.is_empty() {
            continue;
        }

        let user_name = item
            .get("user")
            .and_then(|u| u.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let published_int = item
            .get("published_at_int")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let pub_date = if published_int > 0 {
            util::parse_ms_timestamp(published_int * 1000, 0)
        } else {
            None
        };

        let tags = item
            .get("tag_list")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| t.as_str())
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let html = util::get_html(&article_url).await.ok();
        let mut description = String::new();
        if let Some(body) = html {
            let doc = Html::parse_document(&body);
            if let Ok(sel_body) = Selector::parse(".crayons-article__body") {
                if let Some(el) = doc.select(&sel_body).next() {
                    description = util::element_html(&el);
                }
            }
        }
        if description.is_empty() {
            if let Some(desc) = item.get("description").and_then(|v| v.as_str()) {
                description = desc.to_string();
            }
        }

        items.push(HubItem {
            title,
            description: if description.is_empty() {
                None
            } else {
                Some(description)
            },
            link: Some(article_url),
            author: if user_name.is_empty() {
                None
            } else {
                Some(user_name)
            },
            pub_date,
            categories: tags,
        });
    }

    let feed_title = format!("dev.to top ({})", period);
    let link = format!("{}/top/{}", BASE_URL, period);

    Ok(HubData {
        title: feed_title,
        description: Some("Top dev.to posts by reactions for the selected period.".to_string()),
        link: Some(link),
        image: Some("https://media2.dev.to/dynamic/image/width=32,height=,fit=scale-down,gravity=auto,format=auto/https%3A%2F%2Fdev-to-uploads.s3.amazonaws.com%2Fuploads%2Farticles%2F8j7kvp660rqzt99zui8e.png".to_string()),
        language: Some("en-US".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_DEV_TO_TOP: Route = Route {
    meta: &META_DEV_TO_TOP,
    handler: handler_fn,
};
