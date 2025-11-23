use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use serde_json::Value;

pub const META_YANDE_POST_POPULAR: RouteMeta = RouteMeta {
    hub_id: "yande/post/popular_recent",
    path: "/yande/post/popular_recent/:period?",
    categories: &["picture"],
    example: "/yande/post/popular_recent/1d",
    params: &[ParamMeta {
        name: "period",
        description: "Period: 1d (last 24h), 1w, 1m, 1y.",
        default: Some("1d"),
        options: &[
            ("1d", "Last 24 hours"),
            ("1w", "Last week"),
            ("1m", "Last month"),
            ("1y", "Last year"),
        ],
    }],
    features: Features {
        require_config: &[],
        require_puppeteer: false,
        anti_crawler: false,
        support_bt: false,
        support_podcast: false,
        support_scihub: false,
        nsfw: true,
    },
    radar: &[Radar {
        source: &["yande.re/post"],
        target: "/post/popular_recent",
    }],
    name: "yande.re Popular Recent Posts",
    maintainers: &["captura"],
    url: "https://yande.re",
    description: "yande.re popular recent posts via JSON API, aligned with RSSHub /yande/post/popular_recent route.",
    default_view: Some("pictures"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let period = ctx.param_str("period").unwrap_or("1d");
    let limit = ctx.param_i64("limit").unwrap_or(50).max(1) as usize;

    let api_url = format!(
        "https://yande.re/post/popular_recent.json?period={}",
        period
    );
    let posts: Vec<Value> = util::get_json(&api_url)
        .await
        .map_err(|e| Error::Network(format!("yande.re: {}", e)))?;

    let titles = match period {
        "1w" => "Last week",
        "1m" => "Last month",
        "1y" => "Last year",
        _ => "Last 24 hours",
    };

    let mut items = Vec::new();
    for post in posts.into_iter().take(limit) {
        let id = post.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
        if id == 0 {
            continue;
        }
        let tags = post
            .get("tags")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let author = post
            .get("author")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let created_at = post.get("created_at").and_then(|v| v.as_i64()).unwrap_or(0);
        let sample_url = post
            .get("sample_url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let rating = post
            .get("rating")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let score = post.get("score").and_then(|v| v.as_i64()).unwrap_or(0);
        let source = post
            .get("source")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let parent_id = post.get("parent_id").and_then(|v| v.as_i64()).unwrap_or(0);
        let file_url = post
            .get("file_url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let file_ext = post
            .get("file_ext")
            .and_then(|v| v.as_str())
            .unwrap_or("jpg")
            .to_string();
        let preview_url = post
            .get("preview_url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let mut desc_parts = Vec::new();
        if !sample_url.is_empty() {
            desc_parts.push(format!(r#"<img src="{}" />"#, sample_url));
        }
        desc_parts.push(format!("<p>Rating:{}</p> <p>Score:{}</p>", rating, score));
        if !source.is_empty() {
            desc_parts.push(format!(r#"<a href="{}">Source</a>"#, source));
        }
        if parent_id != 0 {
            desc_parts.push(format!(
                r#"<a href="https://yande.re/post/show/{}">Parent</a>"#,
                parent_id
            ));
        }
        let description = desc_parts.join("");

        let pub_date = if created_at > 0 {
            util::parse_ms_timestamp(created_at * 1000, 0)
        } else {
            None
        };

        let mut categories = Vec::new();
        if !tags.is_empty() {
            categories.extend(tags.split_whitespace().map(|s| s.to_string()));
        }

        // file_url / preview_url are not exposed as enclosure here, but kept in description context.
        let _ = (file_url, file_ext, preview_url);

        items.push(HubItem {
            title: tags.clone(),
            description: if description.is_empty() {
                None
            } else {
                Some(description)
            },
            link: Some(format!("https://yande.re/post/show/{}", id)),
            author: if author.is_empty() {
                None
            } else {
                Some(author)
            },
            pub_date,
            categories,
        });
    }

    Ok(HubData {
        title: format!("{} - yande.re", titles),
        description: Some("yande.re popular recent posts.".to_string()),
        link: Some(format!(
            "https://yande.re/post/popular_recent?period={}",
            period
        )),
        image: None,
        language: Some("en-US".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_YANDE_POST_POPULAR: Route = Route {
    meta: &META_YANDE_POST_POPULAR,
    handler: handler_fn,
};
