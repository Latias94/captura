use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset, NaiveDate};
use scraper::{Html, Selector};

const ROOT_URL: &str = "https://inside.java";

pub const META_JAVA_INSIDE: RouteMeta = RouteMeta {
    hub_id: "java/inside",
    path: "/java/inside/:sort?",
    categories: &["programming"],
    example: "/java/inside",
    params: &[
        ParamMeta {
            name: "sort",
            description: "排序方式：date（默认）、author、tag。",
            default: Some("date"),
            options: &[("date", "Date"), ("author", "Author"), ("tag", "Tag")],
        },
        ParamMeta {
            name: "limit",
            description: "最大文章数量（默认 20）。",
            default: Some("20"),
            options: &[],
        },
    ],
    features: Features::basic(),
    radar: &[Radar {
        source: &["inside.java"],
        target: "/inside/:sort?",
    }],
    name: "Inside Java",
    maintainers: &["captura"],
    url: "https://inside.java/",
    description: "Inside Java 官方博客：Oracle Java 团队成员的新闻与观点。",
    default_view: Some("articles"),
};

fn build_url(sort: &str) -> String {
    match sort {
        "author" => format!("{}/u", ROOT_URL),
        "tag" => format!("{}/tags", ROOT_URL),
        _ => format!("{}/", ROOT_URL),
    }
}

fn parse_inside_date(s: &str) -> Option<DateTime<FixedOffset>> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    // 例如 "November 20, 2025"
    let fmts = ["%B %d, %Y", "%B %e, %Y"];
    for fmt in &fmts {
        if let Ok(naive) = NaiveDate::parse_from_str(s, fmt) {
            if let Some(dt) = naive.and_hms_opt(0, 0, 0) {
                if let Some(offset) = FixedOffset::east_opt(0) {
                    return Some(DateTime::<FixedOffset>::from_naive_utc_and_offset(
                        dt, offset,
                    ));
                }
            }
        }
    }
    None
}

async fn fetch_index(sort: &str) -> Result<String> {
    let url = build_url(sort);
    util::get_html(&url).await
}

fn extract_items(html: &str, limit: usize) -> Result<Vec<HubItem>> {
    let doc = Html::parse_document(html);

    let sel_article = Selector::parse("div#posts article.post")
        .map_err(|e| Error::Parse(format!("java/inside: invalid article selector: {e}")))?;
    let sel_title_link = Selector::parse("div.post-title h2 a.post-link")
        .map_err(|e| Error::Parse(format!("java/inside: invalid title selector: {e}")))?;
    let sel_info = Selector::parse("span.post-info")
        .map_err(|e| Error::Parse(format!("java/inside: invalid info selector: {e}")))?;
    let sel_author = Selector::parse("span.post-info a[href^=\"/u/\"]")
        .map_err(|e| Error::Parse(format!("java/inside: invalid author selector: {e}")))?;
    let sel_tags = Selector::parse("span#post-tags a.tag-small")
        .map_err(|e| Error::Parse(format!("java/inside: invalid tags selector: {e}")))?;

    let mut items = Vec::new();

    for article in doc.select(&sel_article) {
        if items.len() >= limit {
            break;
        }

        let title_el = article.select(&sel_title_link).next();
        let Some(title_el) = title_el else {
            continue;
        };

        let href = title_el.value().attr("href").unwrap_or("");
        if href.is_empty() {
            continue;
        }
        let link = util::absolutize(ROOT_URL, href);

        let title = title_el.text().collect::<String>().trim().to_string();
        if title.is_empty() {
            continue;
        }

        // post-info 中包含作者和日期，作者单独再抓
        let date_text = article
            .select(&sel_info)
            .next()
            .map(|info| info.text().collect::<String>())
            .unwrap_or_default();
        let pub_date = parse_inside_date(&date_text);

        let author = article
            .select(&sel_author)
            .next()
            .map(|a| a.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty());

        let mut categories = Vec::new();
        categories.push("java".to_string());

        for tag in article.select(&sel_tags) {
            let tag_text = tag.text().collect::<String>().trim().to_string();
            if !tag_text.is_empty() {
                categories.push(tag_text);
            }
        }

        items.push(HubItem {
            title,
            description: None,
            link: Some(link),
            author,
            pub_date,
            categories,
        });
    }

    Ok(items)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let sort = ctx.param_str("sort").unwrap_or("date");
    let sort = match sort {
        "author" => "author",
        "tag" => "tag",
        _ => "date",
    };
    let limit = ctx.param_i64("limit").unwrap_or(20).max(1) as usize;

    let html = fetch_index(sort).await?;
    let items = extract_items(&html, limit)?;

    let mut title = "Inside Java".to_string();
    if sort == "author" {
        title.push_str(" - By Author");
    } else if sort == "tag" {
        title.push_str(" - By Tag");
    }

    let link = build_url(sort);

    Ok(HubData {
        title,
        description: Some("Inside Java 官方博客文章列表。".to_string()),
        link: Some(link),
        image: Some("https://inside.java/images/java-logo-vert-blk.png".to_string()),
        language: Some("en".to_string()),
        items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_JAVA_INSIDE: Route = Route {
    meta: &META_JAVA_INSIDE,
    handler: handler_fn,
};
