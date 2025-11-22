use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset};
use serde::Deserialize;

const API_HOME: &str = "https://api.hellogithub.com/v1/";
const ROOT_URL: &str = "https://hellogithub.com";

#[derive(Debug, Deserialize)]
struct HomeItem {
    item_id: String,
    full_name: String,
    title: String,
    #[serde(default)]
    title_en: Option<String>,
    author: String,
    name: String,
    summary: String,
    #[serde(default)]
    primary_lang: Option<String>,
    #[serde(default)]
    updated_at: String,
}

#[derive(Debug, Deserialize)]
struct HomeResp {
    data: Vec<HomeItem>,
}

pub const META_HELLOGITHUB_HOME: RouteMeta = RouteMeta {
    hub_id: "hellogithub/home",
    path: "/hellogithub/home/:sort?/:tag?",
    categories: &["programming"],
    example: "/hellogithub/home",
    params: &[
        ParamMeta {
            name: "sort",
            description: "排序方式：featured（精选）或 all（全部），默认 featured",
            default: Some("featured"),
            options: &[("featured", "精选"), ("all", "全部")],
        },
        ParamMeta {
            name: "tag",
            description: "标签 id，可在 HelloGitHub 标签页 URL 中找到（tid 参数）。",
            default: None,
            options: &[],
        },
        ParamMeta {
            name: "limit",
            description: "最大项目数量（1-50，默认 20）",
            default: Some("20"),
            options: &[],
        },
    ],
    features: Features::basic(),
    radar: &[Radar {
        source: &["hellogithub.com"],
        target: "/",
    }],
    name: "HelloGitHub 开源项目",
    maintainers: &["captura"],
    url: "https://hellogithub.com/",
    description: "HelloGitHub 精选 / 全部开源项目，对标 RSSHub /hellogithub/home 路由。",
    default_view: Some("program-update"),
};

fn parse_pub_date(s: &str) -> Option<DateTime<FixedOffset>> {
    util::parse_date(s)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let sort = ctx.param_str("sort").unwrap_or("featured").trim();
    let tag = ctx.param_str("tag").unwrap_or("").trim();
    let limit = ctx.param_i64("limit").unwrap_or(20).max(1).min(50) as usize;

    let sort = match sort {
        "all" => "all",
        _ => "featured",
    };

    let mut api_url = format!("{}?sort_by={}&page=1", API_HOME, sort);
    if !tag.is_empty() {
        api_url.push_str("&tid=");
        api_url.push_str(tag);
    }

    let resp: HomeResp = util::get_json(&api_url).await?;

    let mut items = Vec::new();
    for item in resp.data.into_iter().take(limit) {
        let repo_name = format!("{}/{}", item.author, item.name);
        let title = format!("{}: {}", repo_name, item.title);
        let link = format!("{}/repository/{}", ROOT_URL, item.item_id);
        let pub_date = parse_pub_date(&item.updated_at);

        let mut desc = String::new();
        desc.push_str(&html_escape::encode_safe(&item.summary));
        if let Some(lang) = &item.primary_lang {
            desc.push_str("<br><strong>Language:</strong> ");
            desc.push_str(&html_escape::encode_safe(lang));
        }

        items.push(HubItem {
            title,
            description: Some(desc),
            link: Some(link),
            author: Some(item.author),
            pub_date,
            categories: vec!["hellogithub".to_string(), "repository".to_string()],
        });
    }

    let mut title = "HelloGitHub 项目".to_string();
    if sort == "featured" {
        title = "HelloGitHub 精选开源项目".to_string();
    }

    let link = format!(
        "{root}/?sort_by={sort}{tid}",
        root = ROOT_URL,
        sort = sort,
        tid = if tag.is_empty() {
            "".to_string()
        } else {
            format!("&tid={}", tag)
        }
    );

    Ok(HubData {
        title: title.clone(),
        description: Some(title),
        link: Some(link),
        image: None,
        language: Some("zh-CN".to_string()),
        items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_HELLOGITHUB_HOME: Route = Route {
    meta: &META_HELLOGITHUB_HOME,
    handler: handler_fn,
};
