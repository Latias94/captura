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

#[derive(Debug, Default, Deserialize)]
struct MessageUser {
    #[serde(default)]
    name: String,
    #[serde(default)]
    username: String,
}

#[derive(Debug, Deserialize)]
struct Message {
    #[serde(default)]
    text: String,
    #[serde(default)]
    created_at: String,
    #[serde(default)]
    like_count: i64,
    #[serde(default)]
    comment_count: i64,
    #[serde(default)]
    user: MessageUser,
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
    user: MessageUser,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type_label")]
enum PostType {
    #[serde(rename = "cast")]
    Cast { cast: CastItem },
    #[serde(rename = "message")]
    Message { message: Message },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
struct PostItem {
    id: String,
    #[serde(flatten)]
    kind: PostType,
}

#[derive(Debug, Deserialize)]
struct PostList {
    #[serde(default)]
    results: Vec<PostItem>,
}

fn parse_date(s: &str) -> Option<DateTime<FixedOffset>> {
    crate::routes::util::parse_date(s)
}

pub const META_OTOBANANA_TIMELINE: RouteMeta = RouteMeta {
    hub_id: "otobanana/timeline",
    path: "/otobanana/user/:id",
    categories: &["multimedia"],
    example: "/otobanana/user/cee16401-96b1-420f-8188-abd4d33093f1",
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
        source: &["otobanana.com/user/:id"],
        target: "/user/:id",
    }],
    name: "OTOBANANA Timeline タイムライン",
    maintainers: &["captura"],
    url: "https://otobanana.com",
    description:
        "OTOBANANA user timeline (casts + messages), aligned with RSSHub /otobanana/user/:id route.",
    default_view: Some("podcast"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let id = ctx
        .param_str("id")
        .ok_or_else(|| Error::Config("otobanana/timeline: missing id parameter".to_string()))?;
    let limit = ctx.param_i64("limit").unwrap_or(50).max(1) as usize;

    let user_url = format!("{API_BASE}/users/{id}/");
    let posts_url = format!("{API_BASE}/users/{id}/posts/");

    let user: User = util::get_json(&user_url).await?;
    let list: PostList = util::get_json(&posts_url).await?;

    let mut items = Vec::new();

    for post in list.results.into_iter().take(limit) {
        match post.kind {
            PostType::Cast { cast } => {
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
                    author: Some(format!("{} (@{})", cast.user.name, cast.user.username)),
                    pub_date,
                    categories: Vec::new(),
                });
            }
            PostType::Message { message } => {
                if message.text.trim().is_empty() {
                    continue;
                }
                let title = message.text.lines().next().unwrap_or("").trim().to_string();
                if title.is_empty() {
                    continue;
                }
                let link = format!("{BASE_URL}/message/{}", post.id);
                let pub_date = parse_date(&message.created_at);
                let description = message.text.replace('\n', "<br>");

                items.push(HubItem {
                    title,
                    description: Some(description),
                    link: Some(link),
                    author: Some(format!(
                        "{} (@{})",
                        message.user.name, message.user.username
                    )),
                    pub_date,
                    categories: Vec::new(),
                });
            }
            PostType::Other => continue,
        }
    }

    Ok(HubData {
        title: format!(
            "{} (@{}) - タイムライン | OTOBANANA",
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
pub const ROUTE_OTOBANANA_TIMELINE: Route = Route {
    meta: &META_OTOBANANA_TIMELINE,
    handler: handler_fn,
};
