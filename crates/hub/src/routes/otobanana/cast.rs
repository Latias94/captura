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
struct CastItem {
    #[serde(default)]
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    created_at: String,
    #[serde(default)]
    thumbnail_url: String,
    #[serde(default)]
    duration_time: String,
    #[serde(default)]
    audio_url: String,
    #[serde(default)]
    like_count: i64,
    #[serde(default)]
    comment_count: i64,
    #[serde(default)]
    user: User,
}

#[derive(Debug, Deserialize)]
struct CastList {
    #[serde(default)]
    results: Vec<CastItem>,
}

fn parse_date(s: &str) -> Option<DateTime<FixedOffset>> {
    crate::routes::util::parse_date(s)
}

pub const META_OTOBANANA_CAST: RouteMeta = RouteMeta {
    hub_id: "otobanana/cast",
    path: "/otobanana/user/:id/cast",
    categories: &["multimedia"],
    example: "/otobanana/user/cee16401-96b1-420f-8188-abd4d33093f1/cast",
    params: &[ParamMeta {
        name: "id",
        description: "User ID from otobanana user URL.",
        default: None,
        options: &[],
    }],
    features: Features {
        require_config: &[],
        require_puppeteer: false,
        anti_crawler: false,
        support_bt: false,
        support_podcast: true,
        support_scihub: false,
        nsfw: false,
    },
    radar: &[Radar {
        source: &["otobanana.com/user/:id/cast", "otobanana.com/user/:id"],
        target: "/user/:id/cast",
    }],
    name: "OTOBANANA Cast 音声投稿",
    maintainers: &["captura"],
    url: "https://otobanana.com",
    description:
        "OTOBANANA user cast posts (audio uploads), aligned with RSSHub /otobanana/user/:id/cast route.",
    default_view: Some("podcast"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let id = ctx
        .param_str("id")
        .ok_or_else(|| Error::Config("otobanana/cast: missing id parameter".to_string()))?;
    let limit = ctx.param_i64("limit").unwrap_or(50).max(1) as usize;

    let user_url = format!("{API_BASE}/users/{id}/");
    let cast_url = format!("{API_BASE}/users/{id}/casts/");

    let user: User = util::get_json(&user_url).await?;
    let list: CastList = util::get_json(&cast_url).await?;

    let mut items = Vec::new();

    for cast in list.results.into_iter().take(limit) {
        let title = cast.title.trim().to_string();
        if title.is_empty() {
            continue;
        }
        let link = format!("{BASE_URL}/cast/{}", cast.id);
        let pub_date = parse_date(&cast.created_at);

        let mut description = String::new();
        if !cast.thumbnail_url.is_empty() {
            description.push_str(&format!(
                "<p><img src=\"{src}\" alt=\"{alt}\"></p>",
                src = cast.thumbnail_url,
                alt = title
            ));
        }
        if !cast.audio_url.is_empty() {
            description.push_str(&format!(
                "<p><audio controls src=\"{src}\">Your browser does not support the audio element.</audio></p>",
                src = cast.audio_url
            ));
        }

        items.push(HubItem {
            title,
            description: if description.is_empty() {
                None
            } else {
                Some(description)
            },
            link: Some(link),
            author: Some(format!("{} (@{})", user.name, user.username)),
            pub_date,
            categories: Vec::new(),
        });
    }

    Ok(HubData {
        title: format!("{} (@{}) - 音声投稿 | OTOBANANA", user.name, user.username),
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
pub const ROUTE_OTOBANANA_CAST: Route = Route {
    meta: &META_OTOBANANA_CAST,
    handler: handler_fn,
};
