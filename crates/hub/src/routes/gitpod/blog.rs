use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;
use chrono::{DateTime, FixedOffset};
use scraper::{Html, Selector};

const ROOT_URL: &str = "https://www.gitpod.io";

pub const META_GITPOD_BLOG: RouteMeta = RouteMeta {
    hub_id: "gitpod/blog",
    path: "/gitpod/blog",
    categories: &["programming"],
    example: "/gitpod/blog",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["gitpod.io/blog", "gitpod.io"],
        target: "/blog",
    }],
    name: "Gitpod Blog",
    maintainers: &["captura"],
    url: "https://www.gitpod.io/blog",
    description: "Official Gitpod blog, in a simplified form aligned with RSSHub /gitpod/blog.",
    default_view: Some("articles"),
};

fn parse_date(raw: &str) -> Option<DateTime<FixedOffset>> {
    util::parse_date(raw)
}

async fn fetch_html(url: &str) -> Result<String> {
    let client = client_basic(None, None).map_err(|e| Error::Network(e.to_string()))?;
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!("{} -> http status {}", url, status)));
    }
    resp.text().await.map_err(|e| Error::Network(e.to_string()))
}

fn extract_list(
    html: &str,
    limit: usize,
) -> Result<Vec<(String, String, Option<DateTime<FixedOffset>>)>> {
    let doc = Html::parse_document(html);
    // Gitpod 博客卡片整体结构会变化，这里使用较宽松的选择器。
    let sel_card = Selector::parse("a[href^=\"/blog\"] h2, a[href^=\"/blog\"] h3")
        .map_err(|e| Error::Parse(format!("gitpod: invalid card selector: {e}")))?;

    let mut out = Vec::new();
    for heading in doc.select(&sel_card).take(limit) {
        let parent = match heading.parent() {
            Some(p) => p,
            None => continue,
        };
        let link_el = match scraper::ElementRef::wrap(parent) {
            Some(el) => el,
            None => continue,
        };
        let href = link_el.value().attr("href").unwrap_or("");
        if href.is_empty() {
            continue;
        }
        let link = util::absolutize(ROOT_URL, href);
        let title = heading.text().collect::<String>().trim().to_string();
        if title.is_empty() {
            continue;
        }

        // 时间选择器尝试匹配 class 中包含 date 的 span。
        let date_text = link_el
            .select(&Selector::parse("span[class*=\"date\"], time").unwrap())
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();
        let pub_date = parse_date(&date_text);

        out.push((title, link, pub_date));
    }

    Ok(out)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let limit = ctx.param_i64("limit").unwrap_or(10).max(1) as usize;
    let url = format!("{}/blog", ROOT_URL);
    let html = fetch_html(&url).await?;
    let list = extract_list(&html, limit)?;

    let mut items = Vec::new();
    for (title, link, pub_date) in list {
        // 简化版：不再抓详情页，只提供标题 + 链接 + 时间。
        items.push(HubItem {
            title,
            description: None,
            link: Some(link),
            author: None,
            pub_date,
            categories: Vec::new(),
        });
    }

    Ok(HubData {
        title: "Gitpod Blog".to_string(),
        description: Some("Official posts from the Gitpod blog.".to_string()),
        link: Some(url),
        image: None,
        language: Some("en-US".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_GITPOD_BLOG: Route = Route {
    meta: &META_GITPOD_BLOG,
    handler: handler_fn,
};
