use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset, NaiveDate};
use scraper::{Html, Selector};

const ROOT_URL: &str = "https://devblogs.microsoft.com";

pub const META_CSHARP_BLOG: RouteMeta = RouteMeta {
    hub_id: "csharp/blog",
    path: "/csharp/blog/:page?",
    categories: &["programming"],
    example: "/csharp/blog",
    params: &[
        ParamMeta {
            name: "page",
            description: "页码，从 1 开始，默认 1。",
            default: Some("1"),
            options: &[],
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
        source: &["devblogs.microsoft.com/dotnet/category/csharp"],
        target: "/blog/:page?",
    }],
    name: "C# 官方博客",
    maintainers: &["captura"],
    url: "https://devblogs.microsoft.com/dotnet/category/csharp/",
    description: ".NET Blog 中的 C# 分类文章列表。",
    default_view: Some("articles"),
};

fn parse_mdy_date(s: &str) -> Option<DateTime<FixedOffset>> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    // 例如 "Nov 17, 2025"
    let fmts = ["%B %d, %Y", "%b %d, %Y", "%B %e, %Y", "%b %e, %Y"];
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

async fn fetch_index(page: i64) -> Result<String> {
    let page = page.max(1);
    let url = if page <= 1 {
        format!("{}/dotnet/category/csharp/", ROOT_URL)
    } else {
        format!("{}/dotnet/category/csharp/page/{}/", ROOT_URL, page)
    };
    util::get_html(&url).await
}

fn extract_items(html: &str, limit: usize) -> Result<Vec<HubItem>> {
    let doc = Html::parse_document(html);

    let sel_article = Selector::parse("section.archive-posts article.post-card")
        .map_err(|e| Error::Parse(format!("csharp/blog: invalid article selector: {e}")))?;
    let sel_title = Selector::parse("h3 a.single-click")
        .map_err(|e| Error::Parse(format!("csharp/blog: invalid title selector: {e}")))?;
    let sel_excerpt = Selector::parse("div.mt-16 p")
        .map_err(|e| Error::Parse(format!("csharp/blog: invalid excerpt selector: {e}")))?;
    let sel_author = Selector::parse("div.d-flex.gap-6.align-items-center span.fs-14")
        .map_err(|e| Error::Parse(format!("csharp/blog: invalid author selector: {e}")))?;
    let sel_date = Selector::parse("header.entry-header div.d-flex.align-items-left.gap-4")
        .map_err(|e| Error::Parse(format!("csharp/blog: invalid date selector: {e}")))?;

    let mut items = Vec::new();

    for article in doc.select(&sel_article) {
        if items.len() >= limit {
            break;
        }

        let title_el = article.select(&sel_title).next();
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

        let summary = article
            .select(&sel_excerpt)
            .next()
            .map(|p| p.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty());

        let author = article
            .select(&sel_author)
            .next()
            .map(|s| s.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty());

        let date_text = article
            .select(&sel_date)
            .next()
            .map(|d| d.text().collect::<String>().trim().to_string())
            .unwrap_or_default();
        let pub_date = parse_mdy_date(&date_text);

        let mut categories = Vec::new();
        categories.push("csharp".to_string());

        items.push(HubItem {
            title,
            description: summary,
            link: Some(link),
            author,
            pub_date,
            categories,
        });
    }

    Ok(items)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let page = ctx.param_i64("page").unwrap_or(1);
    let limit = ctx.param_i64("limit").unwrap_or(20).max(1) as usize;

    let html = fetch_index(page).await?;
    let items = extract_items(&html, limit)?;

    let mut title = "C# - .NET Blog".to_string();
    if page > 1 {
        title.push_str(&format!(" - Page {}", page));
    }

    let url = if page <= 1 {
        format!("{}/dotnet/category/csharp/", ROOT_URL)
    } else {
        format!("{}/dotnet/category/csharp/page/{}/", ROOT_URL, page)
    };

    Ok(HubData {
        title,
        description: Some("C# 相关的 .NET 官方博客文章。".to_string()),
        link: Some(url),
        image: Some(
            "https://devblogs.microsoft.com/dotnet/wp-content/uploads/sites/10/2025/11/csharp14.webp"
                .to_string(),
        ),
        language: Some("en-US".to_string()),
        items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_CSHARP_BLOG: Route = Route {
    meta: &META_CSHARP_BLOG,
    handler: handler_fn,
};
