use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset};
use scraper::{Html, Selector};
use serde::Deserialize;

const API_URL: &str = "https://yomujp.com/wp-json/wp/v2/posts";

#[derive(Debug, Default, Deserialize)]
struct RenderedText {
    #[serde(default)]
    rendered: String,
}

#[derive(Debug, Deserialize)]
struct Post {
    #[serde(default)]
    title: RenderedText,
    #[serde(default)]
    content: RenderedText,
    #[serde(default)]
    date_gmt: String,
    #[serde(default)]
    modified_gmt: String,
    #[serde(default)]
    guid: RenderedText,
    #[serde(default)]
    link: String,
}

fn parse_date(s: &str) -> Option<DateTime<FixedOffset>> {
    crate::routes::util::parse_date(s)
}

fn format_level(level: Option<&str>) -> String {
    let lower = level.unwrap_or("").to_lowercase();
    match lower.as_str() {
        "n6" => "n5l".to_string(),
        "n5l" | "n5" | "n4" | "n3" | "n2" | "n1" => lower,
        _ => String::new(),
    }
}

fn level_categories(level: &str) -> String {
    match level {
        "n6" | "n5l" => "27".to_string(),
        "n5" => "26".to_string(),
        "n4" => "21".to_string(),
        "n3" => "20".to_string(),
        "n2" => "19".to_string(),
        "n1" => "17".to_string(),
        _ => "17,19,20,21,26,27".to_string(),
    }
}

pub const META_YOMUJP_LEVEL: RouteMeta = RouteMeta {
    hub_id: "yomujp/level",
    path: "/yomujp/:level?",
    categories: &["reading"],
    example: "/yomujp/n1",
    params: &[ParamMeta {
        name: "level",
        description: "Japanese reading level n1~n6, empty means all.",
        default: None,
        options: &[
            ("n1", "N1"),
            ("n2", "N2"),
            ("n3", "N3"),
            ("n4", "N4"),
            ("n5", "N5"),
            ("n6", "N5L (easier than N5)"),
        ],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["yomujp.com/", "yomujp.com/:level"],
        target: "/:level",
    }],
    name: "日本語多読道場等级",
    maintainers: &["captura"],
    url: "https://yomujp.com",
    description:
        "Japanese extensive reading site Yomujp by level, aligned with RSSHub /yomujp/:level route.",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let level_raw = ctx.param_str("level");
    let level = format_level(level_raw);
    let per_page = ctx.param_i64("limit").unwrap_or(10).max(1) as usize;

    let categories = level_categories(&level);
    let url = format!(
        "{}?categories={}&per_page={}",
        API_URL, categories, per_page
    );

    let posts: Vec<Post> = util::get_json(&url)
        .await
        .map_err(|e| Error::Network(format!("yomujp: api error: {}", e)))?;

    let mut items = Vec::new();

    for post in posts {
        if post.title.rendered.trim().is_empty() {
            continue;
        }
        let html = post.content.rendered.replace(['\t', '\n', '\r'], "");
        let doc = Html::parse_document(&html);

        let mut description_parts = Vec::new();
        if let Ok(sel_section) = Selector::parse("section") {
            for section in doc.select(&sel_section).skip(2) {
                description_parts.push(section.html());
            }
        }
        let description = if description_parts.is_empty() {
            html
        } else {
            description_parts.join("")
        };

        let pub_date = parse_date(&post.date_gmt);

        items.push(HubItem {
            title: post.title.rendered.clone(),
            description: Some(description),
            link: Some(post.link.clone()),
            author: Some("Yomujp".to_string()),
            pub_date,
            categories: Vec::new(),
        });
    }

    let title = if level.is_empty() {
        "日本語多読道場".to_string()
    } else {
        format!("{} | 日本語多読道場", level.to_uppercase())
    };
    let feed_link = if level.is_empty() {
        "https://yomujp.com/".to_string()
    } else {
        format!("https://yomujp.com/{}", level)
    };

    Ok(HubData {
        title,
        description: Some("Japanese extensive reading materials by level.".to_string()),
        link: Some(feed_link),
        image: Some(
            "https://yomujp.com/wp-content/uploads/2023/08/top1-2-300x99-1.png".to_string(),
        ),
        language: Some("ja-JP".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_YOMUJP_LEVEL: Route = Route {
    meta: &META_YOMUJP_LEVEL,
    handler: handler_fn,
};
