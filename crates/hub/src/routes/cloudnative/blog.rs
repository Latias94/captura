use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset};
use scraper::{Html, Selector};

const ROOT_URL: &str = "https://cloudnative.to";

pub const META_CLOUDNATIVE_BLOG: RouteMeta = RouteMeta {
    hub_id: "cloudnative/blog",
    path: "/cloudnative/blog",
    categories: &["blog"],
    example: "/cloudnative/blog",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["cloudnative.to/blog"],
        target: "/blog",
    }],
    name: "云原生社区博客",
    maintainers: &["captura"],
    url: "https://cloudnative.to/blog/",
    description: "云原生社区（中国）博客文章，对标 RSSHub /cloudnative/blog 路由。",
    default_view: Some("articles"),
};

fn parse_date(raw: &str) -> Option<DateTime<FixedOffset>> {
    // RSSHub 使用 YYYY-MM-DD 并转东八区，我们复用 parse_date。
    util::parse_date(raw)
}

fn extract_items(html: &str, limit: usize) -> Result<Vec<HubItem>> {
    let doc = Html::parse_document(html);
    let sel_item = Selector::parse("div.page-body .stream-item")
        .map_err(|e| Error::Parse(format!("cloudnative: invalid item selector: {e}")))?;
    let sel_title = Selector::parse(".article-title > a")
        .map_err(|e| Error::Parse(format!("cloudnative: invalid title selector: {e}")))?;
    let sel_summary = Selector::parse(".summary-link")
        .map_err(|e| Error::Parse(format!("cloudnative: invalid summary selector: {e}")))?;
    let sel_meta = Selector::parse(".stream-meta .article-metadata")
        .map_err(|e| Error::Parse(format!("cloudnative: invalid metadata selector: {e}")))?;

    let mut items = Vec::new();

    for item in doc.select(&sel_item).take(limit) {
        let title_el = item.select(&sel_title).next();
        let Some(title_el) = title_el else {
            continue;
        };
        let title = title_el.text().collect::<String>().trim().to_string();
        if title.is_empty() {
            continue;
        }
        let href = title_el.value().attr("href").unwrap_or("");
        if href.is_empty() {
            continue;
        }
        let link = util::absolutize(ROOT_URL, href);

        let summary = item
            .select(&sel_summary)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();

        let meta = item.select(&sel_meta).next();
        let (author, pub_date, category) = if let Some(meta_el) = meta {
            let author = meta_el
                .select(&Selector::parse("span").unwrap())
                .next()
                .and_then(|s| s.select(&Selector::parse("a").unwrap()).next())
                .map(|el| el.text().collect::<String>().trim().to_string());
            let date_text = meta_el
                .select(&Selector::parse(".article-date").unwrap())
                .next()
                .map(|el| el.text().collect::<String>().trim().to_string())
                .unwrap_or_default()
                .replace("发布于", "");
            let pub_date = parse_date(&date_text);
            let category = meta_el
                .select(&Selector::parse(".article-categories a").unwrap())
                .next()
                .map(|el| el.text().collect::<String>().trim().to_string());
            (author, pub_date, category)
        } else {
            (None, None, None)
        };

        let mut categories = Vec::new();
        if let Some(cat) = category {
            if !cat.is_empty() {
                categories.push(cat);
            }
        }

        items.push(HubItem {
            title,
            description: if summary.is_empty() {
                None
            } else {
                Some(summary)
            },
            link: Some(link),
            author,
            pub_date,
            categories,
        });
    }

    Ok(items)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let limit = ctx.param_i64("limit").unwrap_or(30).max(1) as usize;
    let url = format!("{}/blog/", ROOT_URL);
    let html = util::get_html(&url).await?;
    let items = extract_items(&html, limit)?;

    Ok(HubData {
        title: "博客 | 云原生社区（中国）".to_string(),
        description: Some("云原生社区（中国）博客文章。".to_string()),
        link: Some(url),
        image: None,
        language: Some("zh-CN".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_CLOUDNATIVE_BLOG: Route = Route {
    meta: &META_CLOUDNATIVE_BLOG,
    handler: handler_fn,
};
