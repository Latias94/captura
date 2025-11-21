use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;
use chrono::{Datelike, FixedOffset, Local, NaiveDateTime, TimeZone};
use serde_json::Value;

pub const META_GAMER_ANI_NEW_ANIME: RouteMeta = RouteMeta {
    hub_id: "gamer/ani/new_anime",
    path: "/gamer/ani/new_anime",
    categories: &["anime"],
    example: "/gamer/ani/new_anime",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["ani.gamer.com.tw"],
        target: "/new_anime",
    }],
    name: "動畫瘋 - 最後更新",
    maintainers: &["captura"],
    url: "https://ani.gamer.com.tw",
    description: "Ani-Gamer latest updates list, aligned with RSSHub /gamer/ani/new_anime route.",
    default_view: Some("videos"),
};

/// Parse Ani-Gamer `upTime` + `upTimeHours` like `MM/DD` + `HH:mm` into CST (+8).
///
/// The upstream RSSHub route uses `parseDate` with format `MM/DD HH:mm`
/// and applies a +8 timezone. We emulate that here by assuming the
/// current year in the local clock when constructing the final datetime.
fn parse_pub_date(date: &str, time: &str) -> Option<chrono::DateTime<FixedOffset>> {
    let date = date.trim();
    let time = time.trim();
    if date.is_empty() || time.is_empty() {
        return None;
    }

    let year = Local::now().year();
    let combined = format!("{year}/{date} {time}");

    let naive = NaiveDateTime::parse_from_str(&combined, "%Y/%m/%d %H:%M").ok()?;
    let offset = FixedOffset::east_opt(8 * 3600)?;
    offset.from_local_datetime(&naive).single()
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let client = client_basic(None, None)
        .map_err(|e| Error::Network(format!("gamer/ani/new_anime client error: {}", e)))?;
    let resp = client
        .get("https://api.gamer.com.tw/mobile_app/anime/v3/index.php")
        .send()
        .await
        .map_err(|e| Error::Network(format!("gamer/ani/new_anime: {}", e)))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "gamer/ani/new_anime: http status {}",
            status
        )));
    }
    let json: Value = resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("gamer/ani/new_anime parse error: {}", e)))?;

    let dates = json
        .get("data")
        .and_then(|d| d.get("newAnime"))
        .and_then(|n| n.get("date"))
        .and_then(|d| d.as_array())
        .ok_or_else(|| {
            Error::Parse("gamer/ani/new_anime: unexpected JSON structure".to_string())
        })?;

    let mut items = Vec::new();
    let limit = ctx.param_i64("limit").unwrap_or(50).max(1) as usize;
    let root_url = "https://ani.gamer.com.tw";

    for v in dates.iter().take(limit) {
        let title = v.get("title").and_then(|t| t.as_str()).unwrap_or("").trim();
        if title.is_empty() {
            continue;
        }

        let volume = match v.get("volume") {
            Some(Value::String(s)) => s.trim().to_string(),
            Some(Value::Number(n)) => n.to_string(),
            _ => String::new(),
        };
        let full_title = if volume.is_empty() {
            title.to_string()
        } else {
            format!("{title} {volume}")
        };

        let cover = v
            .get("cover")
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();
        if cover.is_empty() {
            continue;
        }

        let video_sn = match v.get("videoSn") {
            Some(Value::Number(n)) => n.as_i64(),
            Some(Value::String(s)) => s.parse::<i64>().ok(),
            _ => None,
        };
        let Some(video_sn) = video_sn else {
            continue;
        };

        let up_time = v
            .get("upTime")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();
        let up_time_hours = v
            .get("upTimeHours")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();

        let link = format!("{}/animeVideo.php?sn={}", root_url, video_sn);
        let description = format!(r#"<img src="{}">"#, cover);
        let pub_date = parse_pub_date(&up_time, &up_time_hours);

        items.push(HubItem {
            title: full_title,
            description: Some(description),
            link: Some(link),
            author: None,
            pub_date,
            categories: Vec::new(),
        });
    }

    Ok(HubData {
        title: "動畫瘋最後更新".to_string(),
        description: Some("Ani-Gamer latest episode updates.".to_string()),
        link: Some(root_url.to_string()),
        image: None,
        language: Some("zh-TW".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_GAMER_ANI_NEW_ANIME: Route = Route {
    meta: &META_GAMER_ANI_NEW_ANIME,
    handler: handler_fn,
};
