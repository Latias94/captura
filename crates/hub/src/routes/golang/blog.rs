use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset, NaiveDate};
use scraper::{Html, Selector};

const ROOT_URL: &str = "https://go.dev";

pub const META_GOLANG_BLOG: RouteMeta = RouteMeta {
    hub_id: "golang/blog",
    path: "/golang/blog",
    categories: &["programming"],
    example: "/golang/blog",
    params: &[ParamMeta {
        name: "limit",
        description: "最大文章数量，默认 30",
        default: Some("30"),
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["go.dev/blog"],
        target: "/blog",
    }],
    name: "The Go Blog",
    maintainers: &["captura"],
    url: "https://go.dev/blog/",
    description: "Go 官方博客文章列表。",
    default_view: Some("articles"),
};

fn parse_go_date(s: &str) -> Option<DateTime<FixedOffset>> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    // 形如 "14 November 2025"
    let naive = NaiveDate::parse_from_str(s, "%d %B %Y").ok()?;
    let naive_dt = naive.and_hms_opt(0, 0, 0)?;
    let offset = FixedOffset::east_opt(0)?;
    Some(DateTime::<FixedOffset>::from_naive_utc_and_offset(
        naive_dt, offset,
    ))
}

async fn fetch_index() -> Result<String> {
    util::get_html(&format!("{}/blog/", ROOT_URL)).await
}

fn extract_items(html: &str, limit: usize) -> Result<Vec<HubItem>> {
    let doc = Html::parse_document(html);
    let sel_title = Selector::parse("div#blogindex p.blogtitle")
        .map_err(|e| Error::Parse(format!("golang: invalid title selector: {e}")))?;
    let sel_summary = Selector::parse("div#blogindex p.blogsummary")
        .map_err(|e| Error::Parse(format!("golang: invalid summary selector: {e}")))?;
    let sel_a = Selector::parse("a")
        .map_err(|e| Error::Parse(format!("golang: invalid a selector: {e}")))?;
    let sel_date = Selector::parse("span.date")
        .map_err(|e| Error::Parse(format!("golang: invalid date selector: {e}")))?;

    let titles: Vec<_> = doc.select(&sel_title).collect();
    let summaries: Vec<_> = doc.select(&sel_summary).collect();

    let mut items = Vec::new();
    let max_len = titles.len().min(summaries.len());

    for i in 0..max_len {
        if items.len() >= limit {
            break;
        }

        let title_el = titles[i];
        let Some(a) = title_el.select(&sel_a).next() else {
            continue;
        };
        let href = a.value().attr("href").unwrap_or("");
        if href.is_empty() {
            continue;
        }
        // 跳过 “More articles...” 等非正文链接
        if href == "/blog/all" {
            continue;
        }

        let link = util::absolutize(ROOT_URL, href);
        let title = a.text().collect::<String>().trim().to_string();
        if title.is_empty() {
            continue;
        }

        let date_str = title_el
            .select(&sel_date)
            .next()
            .map(|d| d.text().collect::<String>().trim().to_string())
            .unwrap_or_default();
        let pub_date = parse_go_date(&date_str);

        let summary = summaries
            .get(i)
            .map(|s| s.text().collect::<String>().trim().to_string());

        items.push(HubItem {
            title,
            description: summary.filter(|s| !s.is_empty()),
            link: Some(link),
            author: None,
            pub_date,
            categories: vec!["go".to_string()],
        });
    }

    Ok(items)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let limit = ctx.param_i64("limit").unwrap_or(30).max(1) as usize;
    let html = fetch_index().await?;
    let items = extract_items(&html, limit)?;

    Ok(HubData {
        title: "The Go Blog".to_string(),
        description: Some("Go 官方博客文章列表。".to_string()),
        link: Some(format!("{}/blog/", ROOT_URL)),
        image: Some("https://go.dev/doc/gopher/gopher5logo.jpg".to_string()),
        language: Some("en".to_string()),
        items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_GOLANG_BLOG: Route = Route {
    meta: &META_GOLANG_BLOG,
    handler: handler_fn,
};
