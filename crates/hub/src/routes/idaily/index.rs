use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::Result;
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeZone};

const ROOT_URL: &str = "https://idai.ly";
const API_BASE: &str = "https://idaily-cdn.idailycdn.com";

pub const META_IDAILY: RouteMeta = RouteMeta {
    hub_id: "idaily",
    path: "/idaily/:language?",
    categories: &["reading"],
    example: "/idaily",
    params: &[ParamMeta {
        name: "language",
        description: "Language code, e.g. zh-hans or zh-hant, default zh-hans.",
        default: Some("zh-hans"),
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["idai.ly/"],
        target: "/:language?",
    }],
    name: "iDaily 每日环球视野",
    maintainers: &["captura"],
    url: "https://idai.ly",
    description: "Daily global photo news from iDaily, aligned with RSSHub /idaily route.",
    default_view: Some("pictures"),
};

#[derive(Debug, Default, serde::Deserialize)]
struct IdailyUiSets {
    #[serde(default)]
    caption_subtitle: String,
    #[serde(default)]
    cover_landscape_hd_4k: String,
}

#[derive(Debug, Default, serde::Deserialize)]
struct IdailyTag {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Default, serde::Deserialize)]
struct IdailyItem {
    #[serde(default)]
    guid: i64,
    #[serde(default)]
    title: String,
    #[serde(default)]
    link_share: String,
    #[serde(default)]
    location: String,
    #[serde(default)]
    content: String,
    #[serde(default)]
    pubdate_timestamp: i64,
    #[serde(default)]
    lastupdate_timestamp: i64,
    #[serde(default)]
    ui_sets: IdailyUiSets,
    #[serde(default)]
    tags: Vec<IdailyTag>,
}

fn parse_ts(ts: i64) -> Option<DateTime<FixedOffset>> {
    if ts <= 0 {
        return None;
    }
    let offset = FixedOffset::east_opt(0)?;
    // iDaily 返回的是秒级 Unix 时间戳
    let naive = NaiveDateTime::from_timestamp_opt(ts, 0)?;
    Some(offset.from_utc_datetime(&naive))
}

fn build_api_url(language: &str) -> String {
    let lang = if language.is_empty() {
        "zh-hans"
    } else {
        language
    };
    format!("{}/api/list/v3/iphone/{}", API_BASE, lang)
}

fn build_items(raw: Vec<IdailyItem>, limit: usize) -> Vec<HubItem> {
    raw.into_iter()
        .filter(|item| !item.ui_sets.caption_subtitle.trim().is_empty())
        .take(limit)
        .map(|item| {
            let caption = item.ui_sets.caption_subtitle.trim().to_string();
            let title = if caption.is_empty() {
                item.title.clone()
            } else if item.title.is_empty() {
                caption.clone()
            } else {
                format!("{} - {}", caption, item.title)
            };

            let mut html = String::new();
            if !item.ui_sets.cover_landscape_hd_4k.is_empty() {
                let img = util::absolutize(API_BASE, &item.ui_sets.cover_landscape_hd_4k);
                html.push_str(&format!(
                    "<p><img src=\"{src}\" alt=\"{alt}\"></p>",
                    src = img,
                    alt = caption
                ));
            }
            if !item.content.trim().is_empty() {
                if !html.is_empty() {
                    html.push_str("<p></p>");
                }
                html.push_str(&format!("<p>{}</p>", item.content.trim()));
            }

            let categories = item
                .tags
                .into_iter()
                .filter_map(|t| {
                    let name = t.name.trim();
                    if name.is_empty() {
                        None
                    } else {
                        Some(name.to_string())
                    }
                })
                .collect::<Vec<_>>();

            HubItem {
                title,
                description: if html.is_empty() { None } else { Some(html) },
                link: if item.link_share.is_empty() {
                    None
                } else {
                    Some(item.link_share.clone())
                },
                author: if item.location.trim().is_empty() {
                    None
                } else {
                    Some(item.location.trim().to_string())
                },
                pub_date: parse_ts(item.pubdate_timestamp),
                categories,
            }
        })
        .collect()
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let language = ctx.param_str("language").unwrap_or("zh-hans");
    let limit = ctx.param_i64("limit").unwrap_or(100).max(1) as usize;
    let api_url = build_api_url(language);

    let data: Vec<IdailyItem> = util::get_json(&api_url).await?;
    let items = build_items(data, limit);

    // 再抓取首页，获取标题与描述等元信息
    let html = util::get_html(ROOT_URL).await?;
    let doc = scraper::Html::parse_document(&html);
    let sel_title = scraper::Selector::parse("title").unwrap();
    let sel_desc = scraper::Selector::parse("meta[name=\"description\"]").unwrap();
    let sel_keywords = scraper::Selector::parse("meta[name=\"keywords\"]").unwrap();

    let title = doc
        .select(&sel_title)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .unwrap_or_else(|| "iDaily 每日环球视野".to_string());
    let description = doc
        .select(&sel_desc)
        .next()
        .and_then(|el| el.value().attr("content"))
        .map(|s| s.to_string());
    let subtitle = doc
        .select(&sel_keywords)
        .next()
        .and_then(|el| el.value().attr("content"))
        .map(|s| s.to_string());

    let image = Some(format!("{}/img/idaily/logo_2x.png", ROOT_URL));

    Ok(HubData {
        title,
        description: description.or_else(|| subtitle.clone()),
        link: Some(ROOT_URL.to_string()),
        image,
        language: Some("zh".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_IDAILY: Route = Route {
    meta: &META_IDAILY,
    handler: handler_fn,
};
