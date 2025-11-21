use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset};
use serde::Deserialize;

const API_BASE: &str = "https://api.otobanana.com";
const BASE_URL: &str = "https://otobanana.com";

#[derive(Debug, Default, Deserialize)]
struct User {
    #[serde(default)]
    name: String,
    #[serde(default)]
    username: String,
    #[serde(default)]
    bio: String,
    #[serde(default)]
    avatar_url: String,
}

#[derive(Debug, Deserialize)]
struct LiveItem {
    #[serde(default)]
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    created_at: String,
    #[serde(default)]
    room_url: String,
    #[serde(default)]
    is_open: bool,
    #[serde(default)]
    like_count: i64,
    #[serde(default)]
    comment_count: i64,
    #[serde(default)]
    user: User,
}

#[derive(Debug, Deserialize)]
struct LiveList {
    #[serde(default)]
    results: Vec<LiveItem>,
}

fn parse_date(s: &str) -> Option<DateTime<FixedOffset>> {
    crate::routes::util::parse_date(s)
}

pub const META_OTOBANANA_LIVESTREAM: RouteMeta = RouteMeta {
    hub_id: "otobanana/livestream",
    path: "/otobanana/user/:id/livestream",
    categories: &["multimedia"],
    example: "/otobanana/user/cee16401-96b1-420f-8188-abd4d33093f1/livestream",
    params: &[ParamMeta {
        name: "id",
        description: "User ID from otobanana user URL.",
        default: None,
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["otobanana.com/user/:id/livestream", "otobanana.com/user/:id"],
        target: "/user/:id/livestream",
    }],
    name: "OTOBANANA Livestream ライブ配信",
    maintainers: &["captura"],
    url: "https://otobanana.com",
    description:
        "OTOBANANA user livestream sessions, aligned with RSSHub /otobanana/user/:id/livestream route.",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let id = ctx
        .param_str("id")
        .ok_or_else(|| Error::Config("otobanana/livestream: missing id parameter".to_string()))?;

    let user_url = format!("{API_BASE}/users/{id}/");
    let live_url = format!("{API_BASE}/users/{id}/livestreams/");

    let user: User = util::get_json(&user_url).await?;
    let list: LiveList = util::get_json(&live_url).await?;

    let mut items = Vec::new();

    for live in list.results {
        let title = live.title.trim().to_string();
        if title.is_empty() {
            continue;
        }
        let link = live.room_url.clone();
        let pub_date = parse_date(&live.created_at);

        let description = if live.is_open {
            "配信中のライブ".to_string()
        } else {
            "終了しました".to_string()
        };

        items.push(HubItem {
            title,
            description: Some(description),
            link: Some(link),
            author: Some(format!("{} (@{})", user.name, user.username)),
            pub_date,
            categories: Vec::new(),
        });
    }

    Ok(HubData {
        title: format!(
            "{} (@{}) - ライブ配信 | OTOBANANA",
            user.name, user.username
        ),
        description: Some(user.bio.replace('\n', " ")),
        link: Some(format!("{BASE_URL}/user/{id}")),
        image: Some(user.avatar_url.clone()),
        language: Some("ja".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_OTOBANANA_LIVESTREAM: Route = Route {
    meta: &META_OTOBANANA_LIVESTREAM,
    handler: handler_fn,
};
