use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset};
use scraper::{ElementRef, Html, Selector};

const ROOT_URL: &str = "https://www.javlibrary.com";

pub const META_JAVLIBRARY_NEWENTRIES: RouteMeta = RouteMeta {
    hub_id: "javlibrary/newentries",
    path: "/javlibrary/newentries/:language?",
    categories: &["multimedia"],
    example: "/javlibrary/newentries/ja",
    params: &[ParamMeta {
        name: "language",
        description: "Language code in JavLibrary URLs, e.g. ja (default), en, cn.",
        default: Some("ja"),
        options: &[
            ("ja", "Japanese interface"),
            ("en", "English interface"),
            ("cn", "Chinese interface"),
        ],
    }],
    features: Features {
        require_config: &[],
        // JavLibrary is often protected by Cloudflare and similar WAFs, so a
        // JS-capable crawler (e.g. spider + Chrome) is recommended for best
        // results. We still fall back to plain HTTP when unavailable.
        require_puppeteer: true,
        anti_crawler: true,
        support_bt: false,
        support_podcast: false,
        support_scihub: false,
        nsfw: true,
    },
    radar: &[Radar {
        source: &["www.javlibrary.com"],
        target: "/newentries/:language?",
    }],
    name: "JavLibrary new entries",
    maintainers: &["captura"],
    url: "https://www.javlibrary.com",
    description: "JavLibrary latest entries list (simplified, metadata-only) based on the \
         /vl_newentries.php?list page, aligned with RSSHub /javlibrary/newentries.",
    default_view: Some("videos"),
};

fn is_video_anchor(anchor: &ElementRef<'_>) -> bool {
    let parent_node = match anchor.parent() {
        Some(p) => p,
        None => return false,
    };
    let parent_el = match ElementRef::wrap(parent_node) {
        Some(el) => el,
        None => return false,
    };

    let tag = parent_el.value().name();
    let class_attr = parent_el.value().attr("class").unwrap_or("");
    let has_video_class = class_attr.split_whitespace().any(|c| c == "video");
    has_video_class || tag.eq_ignore_ascii_case("strong")
}

fn extract_items(html: &str, language: &str, limit: usize) -> captura_common::Result<Vec<HubItem>> {
    let doc = Html::parse_document(html);

    let sel_anchor = Selector::parse(".videotextlist a, #video_comments a")
        .map_err(|e| Error::Parse(format!("javlibrary: invalid anchor selector: {e}")))?;
    let sel_textarea = Selector::parse("textarea").map_err(|e| Error::Parse(e.to_string()))?;
    let sel_date = Selector::parse(".date").map_err(|e| Error::Parse(e.to_string()))?;

    let mut items = Vec::new();

    'outer: for a in doc.select(&sel_anchor) {
        if !is_video_anchor(&a) {
            continue;
        }
        if items.len() >= limit {
            break 'outer;
        }

        let href = a.value().attr("href").unwrap_or("").trim();
        if href.is_empty() {
            continue;
        }
        // Normalize relative link like "./?v=abcd" → /{language}/?v=abcd
        let mut path = href.trim_start_matches("./");
        if path.starts_with('/') {
            path = &path[1..];
        }
        let link = format!("{}/{}/{}", ROOT_URL, language, path);

        let title = a.text().collect::<String>().trim().to_string();
        if title.is_empty() {
            continue;
        }

        // Try to find the closest ancestor <table> for metadata (description/date).
        let mut desc = String::new();
        let mut pub_date: Option<DateTime<FixedOffset>> = None;

        if let Some(mut node) = a.parent() {
            while let Some(el) = ElementRef::wrap(node) {
                if el.value().name().eq_ignore_ascii_case("table") {
                    if let Some(t) = el.select(&sel_textarea).next() {
                        desc = t.text().collect::<String>().trim().to_string();
                    }
                    if let Some(d) = el.select(&sel_date).next() {
                        let date_raw = d.text().collect::<String>().trim().to_string();
                        if !date_raw.is_empty() {
                            pub_date = util::parse_date(&date_raw);
                        }
                    }
                    break;
                }
                match node.parent() {
                    Some(p) => node = p,
                    None => break,
                }
            }
        }

        let description = if desc.is_empty() {
            None
        } else {
            Some(format!("<p>{}</p>", desc))
        };

        items.push(HubItem {
            title,
            description,
            link: Some(link),
            author: None,
            pub_date,
            categories: Vec::new(),
        });
    }

    Ok(items)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let language = ctx.param_str("language").unwrap_or("ja");
    let limit = ctx.param_i64("limit").unwrap_or(30).max(1) as usize;
    let url = format!("{}/{}/vl_newentries.php?list", ROOT_URL, language);

    // Use smart crawler first to pass potential WAF/Cloudflare, then fall back
    // to plain HTTP if unavailable.
    let html = util::get_html_smart(&url).await?;
    let items = extract_items(&html, language, limit)?;

    Ok(HubData {
        title: format!("JavLibrary new entries ({})", language),
        description: Some(
            "JavLibrary latest entries list (simplified, metadata only).".to_string(),
        ),
        link: Some(url),
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
pub const ROUTE_JAVLIBRARY_NEWENTRIES: Route = Route {
    meta: &META_JAVLIBRARY_NEWENTRIES,
    handler: handler_fn,
};
