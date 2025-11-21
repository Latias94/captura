use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset, NaiveDate};
use scraper::Html;
use serde::Deserialize;
use serde_json::Value;

const BASE_URL: &str = "https://www.pixiv.net";

// 对齐 RSSHub /pixiv/ranking/:mode/:date? 的 mode 语义。
fn normalize_mode(raw: &str) -> &str {
    match raw {
        // alias -> canonical
        "daily" => "day",
        "weekly" => "week",
        "monthly" => "month",
        "male" => "day_male",
        "female" => "day_female",
        "daily_ai" => "day_ai",
        "original" => "week_original",
        "rookie" => "week_rookie",
        "daily_r18" => "day_r18",
        "daily_r18_ai" => "day_r18_ai",
        "male_r18" => "day_male_r18",
        "female_r18" => "day_female_r18",
        "weekly_r18" => "week_r18",
        "r18g" => "week_r18g",
        other => other,
    }
}

fn mode_label(mode: &str) -> &'static str {
    match mode {
        "day" => "Daily Ranking",
        "week" => "Weekly Ranking",
        "month" => "Monthly Ranking",
        "day_male" => "Male Ranking",
        "day_female" => "Female Ranking",
        "week_original" => "Original Works Ranking",
        "week_rookie" => "Rookie Ranking",
        "day_ai" => "AI-generated Daily Ranking",
        "day_r18" => "R-18 Daily Ranking",
        "day_r18_ai" => "R-18 AI-generated Daily Ranking",
        "day_male_r18" => "R-18 Male Ranking",
        "day_female_r18" => "R-18 Female Ranking",
        "week_r18" => "R-18 Weekly Ranking",
        "week_r18g" => "R-18G Ranking",
        _ => "Ranking",
    }
}

fn mode_query_param(mode: &str) -> Option<&'static str> {
    match mode {
        "day" => Some("daily"),
        "week" => Some("weekly"),
        "month" => Some("monthly"),
        "day_male" => Some("male"),
        "day_female" => Some("female"),
        "day_ai" => Some("daily_ai"),
        "week_original" => Some("original"),
        "week_rookie" => Some("rookie"),
        "day_r18" => Some("daily_r18"),
        "day_r18_ai" => Some("daily_r18_ai"),
        "day_male_r18" => Some("male_r18"),
        "day_female_r18" => Some("female_r18"),
        "week_r18" => Some("weekly_r18"),
        "week_r18g" => Some("r18g"),
        _ => None,
    }
}

fn mode_link(mode: &str) -> &'static str {
    match mode {
        "day" => "https://www.pixiv.net/ranking.php?mode=daily",
        "week" => "https://www.pixiv.net/ranking.php?mode=weekly",
        "month" => "https://www.pixiv.net/ranking.php?mode=monthly",
        "day_male" => "https://www.pixiv.net/ranking.php?mode=male",
        "day_female" => "https://www.pixiv.net/ranking.php?mode=female",
        "day_ai" => "https://www.pixiv.net/ranking.php?mode=daily_ai",
        "week_original" => "https://www.pixiv.net/ranking.php?mode=original",
        "week_rookie" => "https://www.pixiv.net/ranking.php?mode=rookie",
        "day_r18" => "https://www.pixiv.net/ranking.php?mode=daily_r18",
        "day_r18_ai" => "https://www.pixiv.net/ranking.php?mode=daily_r18_ai",
        "day_male_r18" => "https://www.pixiv.net/ranking.php?mode=male_r18",
        "day_female_r18" => "https://www.pixiv.net/ranking.php?mode=female_r18",
        "week_r18" => "https://www.pixiv.net/ranking.php?mode=weekly_r18",
        "week_r18g" => "https://www.pixiv.net/ranking.php?mode=r18g",
        _ => "https://www.pixiv.net/ranking.php",
    }
}

fn parse_date_param(date: &str) -> Result<String> {
    // 期望输入格式为 YYYY-MM-DD，对齐 RSSHub 文档。
    let trimmed = date.trim();
    if trimmed.is_empty() {
        return Err(Error::Parse("pixiv: empty date param".to_string()));
    }
    let d = util::parse_ymd_date(trimmed)
        .ok_or_else(|| Error::Parse(format!("pixiv: invalid date '{}'", trimmed)))?;
    Ok(d.format("%Y%m%d").to_string())
}

#[derive(Debug, Deserialize)]
struct PixivAssign {
    #[serde(default)]
    mode: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    date_range_text: Option<String>,
    #[serde(default)]
    contents: Vec<PixivContent>,
}

#[derive(Debug, Deserialize)]
struct PixivContent {
    title: String,
    date: String,
    tags: Vec<String>,
    url: String,
    #[serde(default)]
    user_name: String,
    #[serde(default)]
    profile_img: String,
    #[serde(default)]
    illust_id: i64,
    #[serde(default)]
    user_id: i64,
    #[serde(default)]
    rank: i64,
    #[serde(default)]
    yes_rank: Option<i64>,
    #[serde(default)]
    rating_count: i64,
    #[serde(default)]
    view_count: i64,
}

fn extract_assign(json: &Value) -> Result<PixivAssign> {
    let assign = json
        .get("props")
        .and_then(|v| v.get("pageProps"))
        .and_then(|v| v.get("assign"))
        .ok_or_else(|| Error::Parse("pixiv: missing props.pageProps.assign".to_string()))?;
    serde_json::from_value(assign.clone())
        .map_err(|e| Error::Parse(format!("pixiv: assign decode error: {}", e)))
}

fn build_items(assign: &PixivAssign, limit: usize) -> Vec<HubItem> {
    let mut items = Vec::new();

    for c in assign.contents.iter().take(limit) {
        let link = if c.illust_id > 0 {
            Some(format!("{}/artworks/{}", BASE_URL, c.illust_id))
        } else {
            None
        };

        let title = if c.rank > 0 {
            format!("#{} {}", c.rank, c.title)
        } else {
            c.title.clone()
        };

        let pub_date = util::parse_jp_datetime(&c.date);

        let mut description = String::new();
        if !c.user_name.is_empty() {
            description.push_str(&format!(
                "<p>Artist: {} (ID: {})</p>",
                c.user_name, c.user_id
            ));
        }
        if c.view_count > 0 || c.rating_count > 0 {
            description.push_str(&format!(
                "<p>Views: {} · Bookmarks: {}</p>",
                c.view_count, c.rating_count
            ));
        }
        if !c.url.is_empty() {
            description.push_str(&format!(
                "<p><img src=\"{}\" referrerpolicy=\"no-referrer\" /></p>",
                c.url
            ));
        }

        let categories = if c.tags.is_empty() {
            Vec::new()
        } else {
            c.tags.clone()
        };

        items.push(HubItem {
            title,
            description: if description.is_empty() {
                None
            } else {
                Some(description)
            },
            link,
            author: if c.user_name.is_empty() {
                None
            } else {
                Some(c.user_name.clone())
            },
            pub_date,
            categories,
        });
    }

    items
}

pub const META_PIXIV_RANKING: RouteMeta = RouteMeta {
    hub_id: "pixiv/ranking",
    path: "/pixiv/ranking/:mode/:date?",
    categories: &["social-media"],
    example: "/pixiv/ranking/day",
    params: &[
        ParamMeta {
            name: "mode",
            description: "Ranking mode, e.g. day, week, month, day_male, day_female, week_original, week_rookie, day_ai, day_r18, week_r18, week_r18g.",
            default: Some("day"),
            options: &[
                ("day", "Daily ranking"),
                ("week", "Weekly ranking"),
                ("month", "Monthly ranking"),
                ("day_male", "Daily male ranking"),
                ("day_female", "Daily female ranking"),
                ("day_ai", "AI daily ranking"),
                ("week_original", "Original works ranking"),
                ("week_rookie", "Rookie ranking"),
                ("day_r18", "R-18 daily ranking"),
                ("day_r18_ai", "R-18 AI daily ranking"),
                ("day_male_r18", "R-18 male ranking"),
                ("day_female_r18", "R-18 female ranking"),
                ("week_r18", "R-18 weekly ranking"),
                ("week_r18g", "R-18G ranking"),
            ],
        },
        ParamMeta {
            name: "date",
            description: "Date in YYYY-MM-DD (optional, defaults to today in JST).",
            default: None,
            options: &[],
        },
    ],
    features: Features::basic(),
    radar: &[Radar {
        source: &["www.pixiv.net/ranking.php"],
        target: "/ranking/:mode/:date?",
    }],
    name: "Pixiv Rankings",
    maintainers: &["captura"],
    url: "https://www.pixiv.net/ranking.php",
    description: "Pixiv illustration rankings, parsed from the official ranking page without login, aligned with RSSHub /pixiv/ranking/:mode/:date?.",
    default_view: Some("pictures"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let raw_mode = ctx.param_str("mode").unwrap_or("day");
    let mode = normalize_mode(raw_mode);
    let mode_param = mode_query_param(mode)
        .ok_or_else(|| Error::Parse(format!("pixiv: unsupported mode '{}'", raw_mode)))?;

    let limit = ctx.param_i64("limit").unwrap_or(50).max(1).min(50) as usize;

    let date_param = if let Some(d) = ctx.param_str("date") {
        Some(parse_date_param(d)?)
    } else {
        None
    };

    let mut url = format!(
        "{}/ranking.php?mode={}&content=illust",
        BASE_URL, mode_param
    );
    if let Some(ref date) = date_param {
        url.push_str("&date=");
        url.push_str(date);
    }

    let html = util::get_html(&url).await?;
    let doc = Html::parse_document(&html);
    let json: Value = util::extract_next_data(&html)?;

    let assign = extract_assign(&json)?;
    let items = build_items(&assign, limit);

    let mode_label = mode_label(mode);
    let date_text = assign
        .date_range_text
        .as_deref()
        .and_then(util::parse_jp_date_only)
        .map(|d| d.format("%Y-%m-%d").to_string());

    let title = if let Some(ref d) = date_text {
        format!("Pixiv {} - {}", mode_label, d)
    } else {
        format!("Pixiv {}", mode_label)
    };

    let description = if let Some(ref d) = date_text {
        Some(format!("Pixiv {} for {}.", mode_label, d))
    } else {
        Some(format!("Pixiv {}.", mode_label))
    };

    Ok(HubData {
        title,
        description,
        link: Some(mode_link(mode).to_string()),
        image: None,
        language: Some("ja".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_PIXIV_RANKING: Route = Route {
    meta: &META_PIXIV_RANKING,
    handler: handler_fn,
};
