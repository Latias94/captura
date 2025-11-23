use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::Result;
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, TimeZone};
use serde::Deserialize;

const API_BASE: &str = "https://radio.cgtn.com/downapiRES/radio/v1/program/historyList";
const ROOT_URL: &str = "https://www.cgtn.com";

fn parse_cgtn_datetime(date: &str, time: &str) -> Option<DateTime<FixedOffset>> {
    // date: "2025-10-19", time: "06:32 - 07:00"
    let date = date.trim();
    if date.is_empty() {
        return None;
    }
    let time_part = time.split_whitespace().next().unwrap_or("").trim();
    if time_part.is_empty() {
        return None;
    }

    let date_naive = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    // 支持 HH:MM 或 HH:MM:SS
    let time_naive = if let Ok(t) = NaiveTime::parse_from_str(time_part, "%H:%M:%S") {
        t
    } else {
        NaiveTime::parse_from_str(time_part, "%H:%M").ok()?
    };
    let dt_naive = NaiveDateTime::new(date_naive, time_naive);
    let offset = FixedOffset::east_opt(8 * 3600)?;
    Some(offset.from_utc_datetime(&dt_naive))
}

#[derive(Debug, Deserialize, Default)]
struct ProgramSeries {
    #[serde(default)]
    title: String,
    #[serde(default)]
    content: String,
}

#[derive(Debug, Deserialize)]
struct CgtnItem {
    #[serde(default)]
    title: String,
    #[serde(default)]
    showDate: String,
    #[serde(default)]
    time: String,
    #[serde(default)]
    programSeries: ProgramSeries,
    #[serde(default)]
    detail: String,
    #[serde(default)]
    programUrl: String,
    #[serde(default)]
    mediaUrl: String,
    #[serde(default)]
    duration: String,
    #[serde(default)]
    programTitle: String,
}

#[derive(Debug, Deserialize)]
struct CgtnResp {
    data: Vec<CgtnItem>,
    #[serde(default)]
    info: String,
    #[serde(default)]
    picurl1: String,
}

fn category_to_code(category: &str) -> i32 {
    match category {
        "ezfm" => 1,
        _ => 5, // 其它统一用 5，与 RSSHub 中的 `other` 对应
    }
}

async fn fetch_programs(category: &str, id: &str) -> Result<CgtnResp> {
    let code = category_to_code(category);
    let url = format!("{API_BASE}/programId{0}_category{1}_page1.json", id, code);
    crate::routes::util::get_json::<CgtnResp>(&url).await
}

pub const META_CGTN_PODCAST: RouteMeta = RouteMeta {
    hub_id: "cgtn/podcast",
    path: "/cgtn/podcast/:category/:id",
    categories: &["traditional-media", "podcast"],
    example: "/cgtn/podcast/ezfm/4",
    params: &[
        ParamMeta {
            name: "category",
            description: "类型名，例如 ezfm（其他类型可在 CGTN 播客 URL 中查到）。",
            default: Some("ezfm"),
            options: &[],
        },
        ParamMeta {
            name: "id",
            description: "播客 ID，可从 CGTN 播客栏目 URL 中获得。",
            default: None,
            options: &[],
        },
        ParamMeta {
            name: "limit",
            description: "最大节目数量（默认 20）。",
            default: Some("20"),
            options: &[],
        },
    ],
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
        source: &["cgtn.com/podcast/column/:category/*/:id"],
        target: "/podcast/:category/:id",
    }],
    name: "CGTN 播客",
    maintainers: &["captura"],
    url: "https://www.cgtn.com/radio/",
    description: "中国环球电视网（CGTN）电台播客节目回放列表，包含音频播放链接，适合订阅英语/双语节目。",
    default_view: Some("podcast"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let category = ctx.param_str("category").unwrap_or("ezfm");
    let id = ctx
        .param_str("id")
        .ok_or_else(|| captura_common::Error::Parse("cgtn/podcast: id is required".to_string()))?;
    let limit = ctx.param_i64("limit").unwrap_or(20).max(1) as usize;

    let resp = fetch_programs(category, id).await?;
    let mut items = Vec::new();

    for item in resp.data.into_iter().take(limit) {
        let title = if !item.title.trim().is_empty() {
            item.title.trim().to_string()
        } else if !item.programSeries.title.trim().is_empty() {
            item.programSeries.title.trim().to_string()
        } else {
            continue;
        };

        let link = if !item.mediaUrl.trim().is_empty() {
            Some(item.mediaUrl.trim().to_string())
        } else {
            None
        };

        let pub_date = parse_cgtn_datetime(&item.showDate, &item.time);

        let mut desc = String::new();

        if !item.mediaUrl.trim().is_empty() {
            desc.push_str(&format!(
                "<p><audio controls src=\"{src}\">Your browser does not support the audio element.</audio></p>",
                src = item.mediaUrl.trim()
            ));
        }

        if !item.programUrl.trim().is_empty() {
            desc.push_str(&format!(
                "<p><img src=\"{src}\" alt=\"{alt}\"></p>",
                src = item.programUrl.trim(),
                alt = title
            ));
        }

        let mut text_html = String::new();
        if !item.programSeries.content.trim().is_empty() {
            text_html.push_str(item.programSeries.content.trim());
        } else if !item.detail.trim().is_empty() {
            text_html.push_str(item.detail.trim());
        }

        if !text_html.is_empty() {
            if !desc.is_empty() {
                desc.push('\n');
            }
            desc.push_str(&text_html);
        }

        let mut categories = Vec::new();
        if !item.programTitle.trim().is_empty() {
            categories.push(item.programTitle.trim().to_string());
        }
        categories.push("CGTN".to_string());
        categories.push("podcast".to_string());

        items.push(HubItem {
            title,
            description: if desc.is_empty() { None } else { Some(desc) },
            link,
            author: None,
            pub_date,
            categories,
        });
    }

    let album_title = if resp.info.trim().is_empty() {
        format!("CGTN Podcast (category: {}, id: {})", category, id)
    } else {
        resp.info.trim().to_string()
    };

    Ok(HubData {
        title: format!("中国环球电视网 CGTN Podcast - {}", album_title),
        description: if resp.info.trim().is_empty() {
            None
        } else {
            Some(resp.info.trim().to_string())
        },
        link: Some(format!("{ROOT_URL}/radio/")),
        image: if resp.picurl1.trim().is_empty() {
            None
        } else {
            Some(resp.picurl1.trim().to_string())
        },
        language: Some("en".to_string()),
        items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_CGTN_PODCAST: Route = Route {
    meta: &META_CGTN_PODCAST,
    handler: handler_fn,
};
