use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;
use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeZone};
use serde::Deserialize;

const BASE_URL: &str = "https://hub.baai.ac.cn";
const API_HOST: &str = "https://hub-api.baai.ac.cn";

pub const META_BAAI_HUB: RouteMeta = RouteMeta {
    hub_id: "baai/hub",
    path: "/baai/hub/:tagId?/:sort?/:range?",
    categories: &["programming"],
    example: "/baai/hub",
    params: &[
        ParamMeta {
            name: "tagId",
            description: "Tag id from BAAI Hub, can be found on tag pages or via the official tag list API.",
            default: None,
            options: &[],
        },
        ParamMeta {
            name: "sort",
            description: "Sort order, e.g. new or readCnt. Defaults to new.",
            default: Some("new"),
            options: &[
                ("new", "Newest stories"),
                ("readCnt", "Most read (requires range)"),
            ],
        },
        ParamMeta {
            name: "range",
            description: "Time range in days when sort=readCnt, e.g. 3 / 7 / 30. Optional.",
            default: None,
            options: &[],
        },
    ],
    features: Features::basic(),
    radar: &[Radar {
        source: &["hub.baai.ac.cn/"],
        target: "/hub/:tagId?/:sort?/:range?",
    }],
    name: "智源社区 Hub",
    maintainers: &["captura"],
    url: "https://hub.baai.ac.cn",
    description: "BAAI Hub story list via official JSON API, simplified version of RSSHub /baai/hub route (stories only, events skipped).",
    default_view: Some("articles"),
};

#[derive(Debug, Deserialize)]
struct TagListResponse {
    code: i32,
    #[serde(default)]
    message: String,
    #[serde(default)]
    data: Vec<BaaiTag>,
}

#[derive(Debug, Deserialize)]
struct BaaiTag {
    id: i64,
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    brief: String,
    #[serde(default)]
    icon_url: String,
}

#[derive(Debug, Deserialize)]
struct StoryListResponse {
    code: i32,
    #[serde(default)]
    message: String,
    #[serde(default)]
    data: Vec<BaaiStoryWrapper>,
}

#[derive(Debug, Deserialize)]
struct BaaiStoryWrapper {
    #[serde(default)]
    story_info: Option<BaaiStoryInfo>,
    #[serde(default)]
    is_event: bool,
    #[serde(default)]
    story_id: i64,
}

#[derive(Debug, Deserialize)]
struct BaaiStoryInfo {
    #[serde(default)]
    id: i64,
    #[serde(default)]
    title: String,
    #[serde(default)]
    user_name: String,
    #[serde(default)]
    created_at: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    cover_url: String,
    #[serde(default)]
    tag_names: Vec<BaaiTagName>,
}

#[derive(Debug, Deserialize)]
struct BaaiTagName {
    #[serde(default)]
    title: String,
    #[serde(default)]
    id: i64,
}

fn parse_created_at(raw: &str) -> Option<DateTime<FixedOffset>> {
    // Examples: "2025-11-21 12:20 分享", "2025-11-21 12:10 发布"
    let cleaned = raw
        .replace("分享", "")
        .replace("发布", "")
        .trim()
        .to_string();
    if cleaned.is_empty() {
        return None;
    }
    if let Ok(naive) = NaiveDateTime::parse_from_str(&cleaned, "%Y-%m-%d %H:%M") {
        let offset = FixedOffset::east_opt(8 * 3600)?;
        return Some(offset.from_utc_datetime(&naive));
    }
    // Fallback to generic parser if format changes.
    crate::routes::util::parse_date(&cleaned)
}

async fn fetch_tags() -> Result<Vec<BaaiTag>> {
    let client = client_basic(None, None)
        .map_err(|e| Error::Network(format!("baai tags client error: {}", e)))?;
    let url = format!("{}/api/v1/tags", API_HOST);
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{url} -> {e}")))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!("{url} -> http status {status}")));
    }
    let parsed: TagListResponse = resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("baai tags json parse: {e}")))?;
    if parsed.code != 0 {
        return Err(Error::Network(format!(
            "baai tags api error {} {}",
            parsed.code, parsed.message
        )));
    }
    Ok(parsed.data)
}

async fn fetch_stories(
    page: i64,
    sort: &str,
    tag_id: Option<&str>,
    range: Option<&str>,
    limit: usize,
) -> Result<Vec<BaaiStoryWrapper>> {
    let client = client_basic(None, None)
        .map_err(|e| Error::Network(format!("baai stories client error: {}", e)))?;

    // Use (&str, String) so we can serialize via `query(&params)`.
    let mut params: Vec<(&str, String)> = Vec::new();
    params.push(("page", page.to_string()));
    params.push(("sort", sort.to_string()));
    if let Some(t) = tag_id {
        if !t.trim().is_empty() {
            params.push(("tag_id", t.trim().to_string()));
        }
    }
    if let Some(r) = range {
        if !r.trim().is_empty() {
            params.push(("time_range", r.trim().to_string()));
        }
    }

    let url = format!("{}/api/v1/story/list", API_HOST);
    let resp = client
        .post(&url)
        .query(&params)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{url} -> {e}")))?;

    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!("{url} -> http status {status}")));
    }

    let parsed: StoryListResponse = resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("baai stories json parse: {e}")))?;
    if parsed.code != 0 {
        return Err(Error::Network(format!(
            "baai stories api error {} {}",
            parsed.code, parsed.message
        )));
    }

    Ok(parsed.data.into_iter().take(limit).collect())
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let tag_id_param = ctx.param_str("tagId").unwrap_or("").trim().to_string();
    let sort_param = ctx.param_str("sort").unwrap_or("new").trim().to_string();
    let range_param = ctx.param_str("range").unwrap_or("").trim().to_string();
    let limit = ctx.param_i64("limit").unwrap_or(30).max(1) as usize;

    let tag_meta = if !tag_id_param.is_empty() {
        let tags = fetch_tags().await.unwrap_or_default();
        tags.into_iter().find(|t| t.id.to_string() == tag_id_param)
    } else {
        None
    };

    let stories = fetch_stories(
        1,
        &sort_param,
        if tag_id_param.is_empty() {
            None
        } else {
            Some(tag_id_param.as_str())
        },
        if range_param.is_empty() {
            None
        } else {
            Some(range_param.as_str())
        },
        limit,
    )
    .await?;

    let mut items = Vec::new();
    for wrapper in stories {
        if wrapper.is_event {
            continue;
        }
        let info = match wrapper.story_info {
            Some(i) => i,
            None => continue,
        };
        if info.title.trim().is_empty() {
            continue;
        }

        let title = info.title.trim().to_string();
        let link = if !info.url.trim().is_empty() {
            info.url.trim().to_string()
        } else if wrapper.story_id != 0 {
            format!("{}/view/{}", BASE_URL, wrapper.story_id)
        } else if info.id != 0 {
            format!("{}/view/{}", BASE_URL, info.id)
        } else {
            BASE_URL.to_string()
        };

        let mut description = String::new();
        if !info.summary.trim().is_empty() {
            description.push_str(&format!("<p>{}</p>", info.summary.trim()));
        }
        if !info.cover_url.trim().is_empty() {
            description.push_str("<p>");
            description.push_str(&crate::routes::util::html_img(
                info.cover_url.trim(),
                &title,
            ));
            description.push_str("</p>");
        }

        let author = if info.user_name.trim().is_empty() {
            None
        } else {
            Some(info.user_name.trim().to_string())
        };

        let pub_date = parse_created_at(&info.created_at);

        let categories = info
            .tag_names
            .iter()
            .filter_map(|t| {
                let name = t.title.trim();
                if name.is_empty() {
                    None
                } else {
                    Some(name.to_string())
                }
            })
            .collect::<Vec<_>>();

        items.push(HubItem {
            title,
            description: if description.is_empty() {
                None
            } else {
                Some(description)
            },
            link: Some(link),
            author,
            pub_date,
            categories,
        });
    }

    let (title, description, image) = if let Some(tag) = tag_meta {
        let t = if tag.title.trim().is_empty() {
            "智源社区".to_string()
        } else {
            format!("{} - 智源社区", tag.title.trim())
        };
        let desc = if !tag.brief.trim().is_empty() {
            Some(tag.brief.trim().to_string())
        } else if !tag.description.trim().is_empty() {
            Some(tag.description.trim().to_string())
        } else {
            Some("智源社区 Hub stories.".to_string())
        };
        (
            t,
            desc,
            if tag.icon_url.is_empty() {
                None
            } else {
                Some(tag.icon_url)
            },
        )
    } else {
        (
            "智源社区 Hub".to_string(),
            Some("Stories from BAAI Hub.".to_string()),
            None,
        )
    };

    let mut link = format!("{}/?sort={}", BASE_URL, sort_param);
    if !tag_id_param.is_empty() {
        link.push_str(&format!("&tag_id={}", tag_id_param));
    }
    if !range_param.is_empty() {
        link.push_str(&format!("&time_range={}", range_param));
    }

    Ok(HubData {
        title,
        description,
        link: Some(link),
        image,
        language: Some("zh-CN".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_BAAI_HUB: Route = Route {
    meta: &META_BAAI_HUB,
    handler: handler_fn,
};
