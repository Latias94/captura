use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset};
use scraper::{Html, Selector};
use serde_json::Value;

const ROOT_URL: &str = "https://m.okjike.com";

pub const META_JIKE_USER: RouteMeta = RouteMeta {
    hub_id: "jike/user",
    path: "/jike/user/:id",
    categories: &["social-media"],
    example: "/jike/user/3EE02BC9-C5B3-4209-8750-4ED1EE0F67BB",
    params: &[ParamMeta {
        name: "id",
        description: "Jike user id, from m.okjike.com/users/:id or web.okjike.com/u/:uid.",
        default: None,
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["web.okjike.com/u/:uid"],
        target: "/user/:id",
    }],
    name: "即刻用户动态",
    maintainers: &["captura"],
    url: "https://m.okjike.com",
    description: "User timeline from Jike (即刻) mobile site, aligned with RSSHub /jike/user route but using only public HTML/JSON.",
    default_view: Some("social"),
};

fn parse_date(raw: &str) -> Option<DateTime<FixedOffset>> {
    crate::routes::util::parse_date(raw)
}

fn get_link(id: &str, typ: &str) -> String {
    match typ {
        "REPOST" => format!("{}/reposts/{}", ROOT_URL, id),
        "MEDIUM" => format!("https://www.okjike.com/medium/{}", id),
        _ => format!("{}/originalPosts/{}", ROOT_URL, id),
    }
}

fn load_page_data(html: &str) -> Result<Value> {
    let doc = Html::parse_document(html);
    let sel = Selector::parse(r#"script[type="application/json"]"#)
        .map_err(|e| Error::Parse(format!("jike: selector error: {e}")))?;
    let script = doc
        .select(&sel)
        .next()
        .ok_or_else(|| Error::Parse("jike: application/json script not found".to_string()))?;
    let json_str = script.text().collect::<String>();
    let v: Value = serde_json::from_str(&json_str)
        .map_err(|e| Error::Parse(format!("jike: invalid JSON: {e}")))?;
    v.get("props")
        .and_then(|p| p.get("pageProps"))
        .cloned()
        .ok_or_else(|| Error::Parse("jike: pageProps missing".to_string()))
}

fn build_items(page: &Value, limit: usize) -> Result<(Vec<HubItem>, String, String, String)> {
    let user = page
        .get("user")
        .ok_or_else(|| Error::Parse("jike: user missing".to_string()))?;
    let screen_name = user
        .get("screenName")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let bio = user
        .get("bio")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let avatar = user
        .get("avatarImage")
        .and_then(|v| v.get("picUrl"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let posts = page
        .get("posts")
        .and_then(|v| v.as_array())
        .ok_or_else(|| Error::Parse("jike: posts array missing".to_string()))?;

    let mut items = Vec::new();
    for item in posts.iter().take(limit) {
        let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let id = item.get("id").and_then(|v| v.as_str()).unwrap_or("");
        if id.is_empty() {
            continue;
        }

        let mut content = item
            .get("content")
            .and_then(|v| v.as_str())
            .or_else(|| {
                item.get("linkInfo")
                    .and_then(|v| v.get("title"))
                    .and_then(|v| v.as_str())
            })
            .or_else(|| {
                item.get("question")
                    .and_then(|v| v.get("title"))
                    .and_then(|v| v.as_str())
            })
            .or_else(|| item.get("title").and_then(|v| v.as_str()))
            .unwrap_or("")
            .to_string();

        content = content.replace('\r', "").replace('\n', "<br>");

        let mut shorten_title = if content.is_empty() {
            "一条动态".to_string()
        } else {
            content
                .replace("<br>", " ")
                .split_whitespace()
                .take(60)
                .collect::<Vec<_>>()
                .join(" ")
        };

        let mut html = String::new();
        if !content.is_empty() {
            html.push_str(&content);
            html.push_str("<br><br>");
        }

        if let Some(link_info) = item.get("linkInfo") {
            if let Some(url) = link_info.get("linkUrl").and_then(|v| v.as_str()) {
                let title = link_info
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or(url);
                html.push_str(&format!(
                    r#"<a href="{url}">{title}</a><br>"#,
                    url = url,
                    title = title
                ));
            }
        }

        if let Some(pictures) = item.get("pictures").and_then(|v| v.as_array()) {
            for pic in pictures {
                if let Some(url) = pic.get("picUrl").and_then(|v| v.as_str()) {
                    html.push_str(&format!(r#"<br><img src="{url}" alt="image">"#, url = url));
                }
            }
        }

        if item_type == "REPOST" {
            if let Some(target) = item.get("target") {
                let screen_name = target
                    .get("user")
                    .and_then(|u| u.get("screenName"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let username = target
                    .get("user")
                    .and_then(|u| u.get("username"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let target_content = target.get("content").and_then(|v| v.as_str()).unwrap_or("");

                let mut repost = String::new();
                if !screen_name.is_empty() && !username.is_empty() {
                    repost.push_str(&format!(
                        r#"转发 <a href="{root}/users/{username}" target="_blank">@{screen}</a>: "#,
                        root = ROOT_URL,
                        username = username,
                        screen = screen_name
                    ));
                }
                repost.push_str(&target_content.replace('\r', "").replace('\n', "<br>"));

                if let Some(tpics) = target.get("pictures").and_then(|v| v.as_array()) {
                    for pic in tpics {
                        if let Some(url) = pic.get("thumbnailUrl").and_then(|v| v.as_str()) {
                            repost.push_str(&format!(
                                r#"<br><img src="{url}" alt="image">"#,
                                url = url
                            ));
                        }
                    }
                }

                html.push_str(&format!(r#"<div class="rsshub-quote">{}</div>"#, repost));
            }
        }

        let title = format!("{}了: {}", map_type(item_type), shorten_title);
        let link = get_link(id, item_type);
        let created_at = item.get("createdAt").and_then(|v| v.as_str()).unwrap_or("");
        let pub_date = parse_date(created_at);

        items.push(HubItem {
            title,
            description: Some(html.trim_end_matches("<br>").to_string()),
            link: Some(link),
            author: Some(screen_name.clone()),
            pub_date,
            categories: Vec::new(),
        });
    }

    Ok((items, screen_name, bio, avatar))
}

fn map_type(t: &str) -> &'static str {
    match t {
        "ORIGINAL_POST" => "发布",
        "REPOST" => "转发",
        "ANSWER" => "回答",
        "QUESTION" => "提问",
        "PERSONAL_UPDATE" => "创建新主题",
        _ => "发布",
    }
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let id = ctx.param_str("id").unwrap_or("").trim().to_string();
    if id.is_empty() {
        return Err(captura_common::Error::Parse("id is required".to_string()));
    }
    let limit = ctx.param_i64("limit").unwrap_or(30).max(1) as usize;

    let url = format!("{}/users/{}", ROOT_URL, id);
    let html = util::get_html(&url).await?;
    let page = load_page_data(&html)?;
    let (items, screen_name, bio, avatar) = build_items(&page, limit)?;

    let title = if screen_name.is_empty() {
        format!("{} 的即刻动态", id)
    } else {
        format!("{} 的即刻动态", screen_name)
    };

    Ok(HubData {
        title,
        description: if bio.is_empty() { None } else { Some(bio) },
        link: Some(url),
        image: if avatar.is_empty() {
            None
        } else {
            Some(avatar)
        },
        language: Some("zh-CN".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_JIKE_USER: Route = Route {
    meta: &META_JIKE_USER,
    handler: handler_fn,
};
