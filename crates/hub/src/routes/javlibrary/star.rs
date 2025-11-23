use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset};
use scraper::{ElementRef, Html, Selector};

const ROOT_URL: &str = "https://www.javlibrary.com";

pub const META_JAVLIBRARY_STAR: RouteMeta = RouteMeta {
    hub_id: "javlibrary/star",
    path: "/javlibrary/star/:id/:language?/:mode?",
    categories: &["multimedia"],
    example: "/javlibrary/star/abbds/en",
    params: &[
        ParamMeta {
            name: "id",
            description: "演员 ID，可在 JavLibrary 明星页 URL 中找到，例如 https://www.javlibrary.com/en/vl_star.php?s=abbds。",
            default: None,
            options: &[],
        },
        ParamMeta {
            name: "language",
            description: "界面语言代码：ja（默认）、en、cn。",
            default: Some("ja"),
            options: &[
                ("ja", "Japanese interface"),
                ("en", "English interface"),
                ("cn", "Chinese interface"),
            ],
        },
        ParamMeta {
            name: "mode",
            description: "展示模式：1=按日期带评论，2=按日期所有作品（与 RSSHub 一致）。",
            default: Some("1"),
            options: &[
                ("1", "videos with comments (by date)"),
                ("2", "everything (by date)"),
            ],
        },
    ],
    features: Features {
        require_config: &[],
        require_puppeteer: true,
        anti_crawler: true,
        support_bt: false,
        support_podcast: false,
        support_scihub: false,
        nsfw: true,
    },
    radar: &[Radar {
        source: &["www.javlibrary.com/*"],
        target: "/star/:id/:language?",
    }],
    name: "JavLibrary 按演员列出作品",
    maintainers: &["captura"],
    url: "https://www.javlibrary.com",
    description: "JavLibrary 某位演员的作品列表（简化版，仅元数据），对齐 RSSHub /javlibrary/star/:id/:language?/:mode?。",
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
        .map_err(|e| Error::Parse(format!("javlibrary star: invalid anchor selector: {e}")))?;
    let sel_textarea = Selector::parse("textarea").map_err(|e| Error::Parse(format!("{e}")))?;
    let sel_date = Selector::parse(".date").map_err(|e| Error::Parse(format!("{e}")))?;

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
    let id = ctx.param_str("id").ok_or_else(|| {
        captura_common::Error::Parse("javlibrary/star: id is required".to_string())
    })?;
    let language = ctx.param_str("language").unwrap_or("ja");
    let mode = ctx.param_str("mode").unwrap_or("1");
    let limit = ctx.param_i64("limit").unwrap_or(30).max(1) as usize;

    let url = format!(
        "{}/{}/vl_star.php?list&s={}&mode={}",
        ROOT_URL, language, id, mode
    );

    // 使用 smart 爬虫优先尝试通过 Cloudflare / WAF，再回退到普通 HTTP。
    let html = util::get_html_smart(&url).await?;
    let items = extract_items(&html, language, limit)?;

    Ok(HubData {
        title: format!("JavLibrary star {} ({})", id, language),
        description: Some(
            "JavLibrary 指定演员的作品列表（基于列表页元信息，未抓详情页）。".to_string(),
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
pub const ROUTE_JAVLIBRARY_STAR: Route = Route {
    meta: &META_JAVLIBRARY_STAR,
    handler: handler_fn,
};
