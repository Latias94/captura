use crate::routes::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;
use chrono::{DateTime, FixedOffset, NaiveDate, TimeZone};
use regex::Regex;
use scraper::{Html, Selector};

const ROOT_URL: &str = "https://developer.android.com";
const CURRENT_URL: &str = "https://developer.android.com/studio/releases/platform-tools";

pub const META_ANDROID_PLATFORM_TOOLS_RELEASES: RouteMeta = RouteMeta {
    hub_id: "android/platform-tools-releases",
    path: "/android/platform-tools-releases",
    categories: &["program-update"],
    example: "/android/platform-tools-releases",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &[
            "developer.android.com/studio/releases/platform-tools",
            "developer.android.com/",
        ],
        target: "/studio/releases/platform-tools",
    }],
    name: "SDK Platform Tools release notes",
    maintainers: &["captura"],
    url: "https://developer.android.com/studio/releases/platform-tools",
    description: "Android SDK Platform Tools release notes, roughly aligned with RSSHub /android/platform-tools-releases.",
    default_view: Some("program-update"),
};

fn parse_month_year_from_title(title: &str) -> Option<DateTime<FixedOffset>> {
    // Try to extract the part inside parentheses, e.g. "Android SDK Platform-Tools (January 2025)".
    let re = Regex::new(r"\((?P<inner>.+?)\)").ok()?;
    let caps = re.captures(title)?;
    let inner = caps.name("inner")?.as_str().trim();

    // First try generic date parser (handles full dates if present).
    if let Some(dt) = crate::routes::util::parse_date(inner) {
        return Some(dt);
    }

    // Fallback: month name + year, e.g. "January 2025".
    if let Ok(date) = NaiveDate::parse_from_str(inner, "%B %Y") {
        if let Some(naive) = date.and_hms_opt(0, 0, 0) {
            if let Some(offset) = FixedOffset::east_opt(0) {
                return Some(offset.from_utc_datetime(&naive));
            }
        }
    }
    None
}

pub async fn handler(_ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let client = client_basic(None, None)
        .map_err(|e| Error::Network(format!("android platform-tools client error: {}", e)))?;

    let resp = client
        .get(CURRENT_URL)
        // Android dev docs use a simple sign-in cookie; reuse RSSHub's best-effort value.
        .header("Cookie", "signin=autosignin")
        .send()
        .await
        .map_err(|e| Error::Network(format!("android platform-tools: {}", e)))?;
    if !resp.status().is_success() {
        return Err(Error::Network(format!(
            "android platform-tools: http status {}",
            resp.status()
        )));
    }
    let html = resp
        .text()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;

    let doc = Html::parse_document(&html);

    // Each release section is an <h4> heading followed by paragraphs / lists
    // until the next <h4>. For now we expose title + anchor link and rely on
    // the detailed page for full notes.
    let sel_h4 = Selector::parse("h4").map_err(|e| Error::Parse(e.to_string()))?;

    let mut items = Vec::new();
    for el in doc.select(&sel_h4) {
        let title_attr = el
            .value()
            .attr("data-text")
            .unwrap_or("")
            .trim()
            .to_string();
        let mut title = if title_attr.is_empty() {
            el.text().collect::<String>().trim().to_string()
        } else {
            title_attr
        };
        if title.is_empty() {
            continue;
        }

        let id = el.value().attr("id").unwrap_or("").trim();
        if id.is_empty() {
            continue;
        }

        let link = format!("{}#{}", CURRENT_URL, id);
        let pub_date = parse_month_year_from_title(&title);

        // Very lightweight description: just echo the title; clients can
        // navigate to the anchored section for full content.
        let description = Some(format!("<p>{}</p>", title));

        // Normalize title for feed display (strip parentheses-only suffix when possible).
        if let Some(idx) = title.find('(') {
            let base = title[..idx].trim();
            if !base.is_empty() {
                title = base.to_string();
            }
        }

        items.push(HubItem {
            title,
            description,
            link: Some(link),
            author: None,
            pub_date,
            categories: vec!["android".to_string(), "platform-tools".to_string()],
        });
    }

    let page_title = doc
        .select(&Selector::parse("title").map_err(|e| Error::Parse(e.to_string()))?)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .unwrap_or_else(|| "Android SDK Platform Tools release notes".to_string());

    Ok(HubData {
        title: page_title,
        description: Some("Android SDK Platform Tools release notes.".to_string()),
        link: Some(CURRENT_URL.to_string()),
        image: None,
        language: Some("en".to_string()),
        items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_ANDROID_PLATFORM_TOOLS_RELEASES: Route = Route {
    meta: &META_ANDROID_PLATFORM_TOOLS_RELEASES,
    handler: handler_fn,
};
