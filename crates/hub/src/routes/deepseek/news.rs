use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset, NaiveDate, NaiveTime, TimeZone};
use scraper::{Html, Selector};

const ROOT_URL: &str = "https://api-docs.deepseek.com/zh-cn";

pub const META_DEEPSEEK_NEWS: RouteMeta = RouteMeta {
    hub_id: "deepseek/news",
    path: "/deepseek/news",
    categories: &["programming"],
    example: "/deepseek/news",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["api-docs.deepseek.com"],
        target: "/news",
    }],
    name: "DeepSeek 新闻",
    maintainers: &["captura"],
    url: "https://api-docs.deepseek.com/zh-cn",
    description: "DeepSeek 文档站点中的新闻更新，对标 RSSHub /deepseek/news 路由。",
    default_view: Some("articles"),
};

fn parse_pub_date(raw: &str) -> Option<DateTime<FixedOffset>> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    // DeepSeek 日期类似 `2025-01-15`，按零点 UTC 处理。
    if let Ok(date) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        let time = NaiveTime::from_hms_opt(0, 0, 0)?;
        let naive = date.and_time(time);
        let offset = FixedOffset::east_opt(0)?;
        return Some(offset.from_utc_datetime(&naive));
    }
    None
}

fn extract_items(html: &str) -> Result<Vec<(String, String, Option<DateTime<FixedOffset>>)>> {
    let doc = Html::parse_document(html);
    // 对应 NEWS_LIST_SELECTOR = 'ul.menu__list > li:nth-child(2) ul > li.theme-doc-sidebar-item-link'
    let sel_li =
        Selector::parse("ul.menu__list > li:nth-child(2) ul > li.theme-doc-sidebar-item-link")
            .map_err(|e| Error::Parse(format!("deepseek: invalid list selector: {e}")))?;
    let sel_a = Selector::parse("a")
        .map_err(|e| Error::Parse(format!("deepseek: invalid a selector: {e}")))?;

    let mut items = Vec::new();
    for li in doc.select(&sel_li) {
        let Some(a) = li.select(&sel_a).next() else {
            continue;
        };
        let href = a.value().attr("href").unwrap_or("");
        if href.is_empty() {
            continue;
        }
        let full = util::absolutize(ROOT_URL, href);
        let text = a.text().collect::<String>().trim().to_string();
        if text.is_empty() {
            continue;
        }
        // 文本末尾通常带日期，如 "更新说明 2025-01-15"
        let mut parts = text.rsplitn(2, ' ');
        let last = parts.next().unwrap_or("");
        let maybe_date = parse_pub_date(last);
        let title = if maybe_date.is_some() {
            parts.next().unwrap_or("").trim().to_string()
        } else {
            text.clone()
        };
        items.push((title, full, maybe_date));
    }
    Ok(items)
}

fn extract_article(html: &str, fallback_title: &str) -> (String, Option<String>) {
    // ARTICLE_CONTENT_SELECTOR = '.theme-doc-markdown > div > div'
    let doc = Html::parse_document(html);
    let sel_container = match Selector::parse(".theme-doc-markdown > div > div") {
        Ok(s) => s,
        Err(_) => return (fallback_title.to_string(), None),
    };
    let sel_h1 = Selector::parse("h1").ok();

    let mut title = fallback_title.to_string();
    let mut description = None;

    if let Some(container) = doc.select(&sel_container).next() {
        // 提取并从 HTML 中去掉第一个 h1 作为标题
        if let Some(sel_h1) = sel_h1 {
            if let Some(h1) = container.select(&sel_h1).next() {
                let t = h1.text().collect::<String>().trim().to_string();
                if !t.is_empty() {
                    title = t;
                }
            }
        }
        let html = util::element_html(&container);
        if !html.trim().is_empty() {
            description = Some(html);
        }
    }

    (title, description)
}

pub async fn handler(_ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let html = util::get_html(ROOT_URL).await?;
    let list = extract_items(&html)?;

    let mut items_out = Vec::new();
    for (fallback_title, link, pub_date) in list {
        let mut title = fallback_title.clone();
        let mut description = None;

        if let Ok(article_html) = util::get_html(&link).await {
            let (t, desc) = extract_article(&article_html, &fallback_title);
            title = t;
            description = desc;
        }

        items_out.push(HubItem {
            title,
            description,
            link: Some(link),
            author: None,
            pub_date,
            categories: Vec::new(),
        });
    }

    Ok(HubData {
        title: "DeepSeek 新闻".to_string(),
        description: Some("DeepSeek 文档站点的新闻与更新。".to_string()),
        link: Some(ROOT_URL.to_string()),
        image: None,
        language: None,
        items: items_out,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_DEEPSEEK_NEWS: Route = Route {
    meta: &META_DEEPSEEK_NEWS,
    handler: handler_fn,
};
