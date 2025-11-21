use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct BgmUser {
    nickname: String,
}

#[derive(Debug, Deserialize)]
struct BgmSubjectImages {
    large: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BgmSubject {
    name: String,
    #[serde(default)]
    name_cn: String,
    #[serde(default)]
    eps: i64,
    #[serde(default)]
    score: f64,
    #[serde(default)]
    date: String,
    #[serde(default)]
    images: Option<BgmSubjectImages>,
}

#[derive(Debug, Deserialize)]
struct BgmCollectionItem {
    subject_id: i64,
    #[serde(default)]
    #[allow(dead_code)]
    subject_type: i32,
    #[serde(default)]
    #[allow(dead_code)]
    ep_status: i32,
    #[serde(default)]
    #[allow(dead_code)]
    r#type: i32,
    #[serde(default)]
    updated_at: String,
    subject: BgmSubject,
}

#[derive(Debug, Deserialize)]
struct BgmCollectionResp {
    data: Vec<BgmCollectionItem>,
}

const API_USER: &str = "https://api.bgm.tv/v0/users";
const API_COLLECTIONS: &str = "https://api.bgm.tv/v0/users";

pub const META_BANGUMI_USER_COLLECTIONS: RouteMeta = RouteMeta {
    hub_id: "bangumi.tv/user_collections",
    path: "/bangumi.tv/user/collections/:id/:subject_type?/:status_type?",
    categories: &["anime"],
    example: "/bangumi.tv/user/collections/sai/2/1",
    params: &[
        ParamMeta {
            name: "id",
            description: "Bangumi user id (username), from user page URL.",
            default: None,
            options: &[],
        },
        ParamMeta {
            name: "subject_type",
            description:
                "Subject type: 1=book, 2=anime, 3=music, 4=game, 6=real, or 'all' for all types.",
            default: Some("all"),
            options: &[
                ("all", "All types"),
                ("1", "Book"),
                ("2", "Anime"),
                ("3", "Music"),
                ("4", "Game"),
                ("6", "Real"),
            ],
        },
        ParamMeta {
            name: "status_type",
            description:
                "Collection status: 1=wish, 2=done, 3=doing, 4=on_hold, 5=dropped, or 'all'.",
            default: Some("all"),
            options: &[
                ("all", "All statuses"),
                ("1", "Wish"),
                ("2", "Done"),
                ("3", "Doing"),
                ("4", "On hold"),
                ("5", "Dropped"),
            ],
        },
    ],
    features: Features::basic(),
    radar: &[
        Radar {
            source: &["bgm.tv/anime/list/:id", "bangumi.tv/anime/list/:id"],
            target: "/user/collections/:id/all/all",
        },
        Radar {
            source: &["bgm.tv/anime/list/:id/wish", "bangumi.tv/anime/list/:id/wish"],
            target: "/user/collections/:id/2/1",
        },
    ],
    name: "Bangumi 用户收藏列表",
    maintainers: &["captura"],
    url: "https://bangumi.tv",
    description:
        "Bangumi.tv user collection list via official v0 API, roughly aligned with RSSHub /bangumi.tv/user/collections/:id/:subjectType/:type route.",
    default_view: Some("articles"),
};

fn subject_type_name(code: &str) -> &'static str {
    match code {
        "1" => "书籍",
        "2" => "动画",
        "3" => "音乐",
        "4" => "游戏",
        "6" => "三次元",
        _ => "条目",
    }
}

fn status_type_name(code: &str, subject_type: &str) -> &'static str {
    match subject_type {
        "1" => match code {
            "1" => "想读",
            "2" => "读过",
            "3" => "在读",
            "4" => "搁置",
            "5" => "抛弃",
            _ => "收藏",
        },
        "3" => match code {
            "1" => "想听",
            "2" => "听过",
            "3" => "在听",
            "4" => "搁置",
            "5" => "抛弃",
            _ => "收藏",
        },
        "4" => match code {
            "1" => "想玩",
            "2" => "玩过",
            "3" => "在玩",
            "4" => "搁置",
            "5" => "抛弃",
            _ => "收藏",
        },
        _ => match code {
            "1" => "想看",
            "2" => "看过",
            "3" => "在看",
            "4" => "搁置",
            "5" => "抛弃",
            _ => "收藏",
        },
    }
}

fn build_description_fields(user_nickname: &str, subject_type: &str, status_type: &str) -> String {
    let st = if subject_type == "all" {
        ""
    } else {
        subject_type_name(subject_type)
    };
    let ty = if status_type == "all" {
        ""
    } else {
        status_type_name(status_type, subject_type)
    };

    if !ty.is_empty() && !st.is_empty() {
        format!("{user}{ty}的{st}列表", user = user_nickname)
    } else if !ty.is_empty() {
        format!("{user}{ty}的列表", user = user_nickname)
    } else if !st.is_empty() {
        format!("{user}收藏的{st}列表", user = user_nickname)
    } else {
        format!("{user}的Bangumi收藏列表", user = user_nickname)
    }
}

fn parse_updated_at(s: &str) -> Option<DateTime<FixedOffset>> {
    util::parse_date(s)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let user_id = ctx
        .param_str("id")
        .ok_or_else(|| Error::Config("bangumi.tv/user_collections: missing user id".to_string()))?;
    let subject_type = ctx.param_str("subject_type").unwrap_or("all");
    let status_type = ctx.param_str("status_type").unwrap_or("all");
    let limit = ctx.param_i64("limit").unwrap_or(50).max(1) as usize;

    let user_url = format!("{}/{}", API_USER, user_id);
    let user: BgmUser = util::get_json(&user_url)
        .await
        .map_err(|e| Error::Network(format!("bangumi.tv user api error: {}", e)))?;
    let nickname = user.nickname;

    let mut query_parts = Vec::new();
    if subject_type != "all" {
        query_parts.push(format!("subject_type={}", subject_type));
    }
    if status_type != "all" {
        query_parts.push(format!("type={}", status_type));
    }
    let query = if query_parts.is_empty() {
        String::new()
    } else {
        format!("?{}", query_parts.join("&"))
    };

    let collections_url = format!("{}/{}/collections{}", API_COLLECTIONS, user_id, query);
    let collections: BgmCollectionResp = util::get_json(&collections_url)
        .await
        .map_err(|e| Error::Network(format!("bangumi.tv collections api error: {}", e)))?;

    let desc_suffix = build_description_fields(&nickname, &subject_type, &status_type);

    let mut items = Vec::new();
    for item in collections.data.into_iter().take(limit) {
        let subj = item.subject;
        let title_text = if subj.name_cn.trim().is_empty() {
            subj.name.clone()
        } else {
            subj.name_cn.clone()
        };
        let link = format!("https://bgm.tv/subject/{}", item.subject_id);

        let mut desc = String::new();
        if let Some(img) = subj.images.as_ref().and_then(|i| i.large.as_ref()) {
            desc.push_str(&util::html_img(img, &title_text));
        }
        if subj.eps > 0 {
            desc.push_str(&format!("<p>Eps: {}</p>", subj.eps));
        }
        if subj.score > 0.0 {
            desc.push_str(&format!("<p>Score: {:.1}</p>", subj.score));
        }
        if !subj.date.trim().is_empty() {
            desc.push_str(&format!("<p>Date: {}</p>", subj.date));
        }

        let pub_date = parse_updated_at(&item.updated_at);

        items.push(HubItem {
            title: title_text,
            description: if desc.is_empty() { None } else { Some(desc) },
            link: Some(link),
            author: Some(nickname.clone()),
            pub_date,
            categories: vec![
                "Bangumi".to_string(),
                "Anime".to_string(),
                "Collections".to_string(),
            ],
        });
    }

    let title = desc_suffix.clone();
    let link = format!("https://bgm.tv/user/{}/collections", user_id);

    Ok(HubData {
        title,
        description: Some(desc_suffix),
        link: Some(link),
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
pub const ROUTE_BANGUMI_USER_COLLECTIONS: Route = Route {
    meta: &META_BANGUMI_USER_COLLECTIONS,
    handler: handler_fn,
};
