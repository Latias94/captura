use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset, TimeZone};
use scraper::{Html, Selector};

const ROOT_URL: &str = "https://studygolang.com";

pub const META_STUDYGOLANG_GO: RouteMeta = RouteMeta {
    hub_id: "studygolang/go",
    path: "/studygolang/go/:id?",
    categories: &["programming"],
    example: "/studygolang/go/daily",
    params: &[ParamMeta {
        name: "id",
        description: "板块 id，默认为 weekly，例如：daily、weekly 等。",
        default: Some("weekly"),
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["studygolang.com/go/:id", "studygolang.com"],
        target: "/go/:id",
    }],
    name: "Go 语言中文网板块",
    maintainers: &["captura"],
    url: "https://studygolang.com/",
    description: "Go 语言中文网各板块主题列表，对标 RSSHub /studygolang/go/:id 路由的精简实现。",
    default_view: Some("articles"),
};

fn parse_pub_date(s: &str) -> Option<DateTime<FixedOffset>> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    util::parse_date(s)
}

async fn fetch_topic_detail(link: &str) -> Result<(Option<DateTime<FixedOffset>>, Option<String>)> {
    let html = util::get_html(link).await?;
    let doc = Html::parse_document(&html);
    let sel_time = Selector::parse(".timeago")
        .map_err(|e| Error::Parse(format!("studygolang: invalid time selector: {e}")))?;
    let sel_content = Selector::parse(".content")
        .map_err(|e| Error::Parse(format!("studygolang: invalid content selector: {e}")))?;

    let time_str = doc.select(&sel_time).next().and_then(|el| {
        if let Some(attr) = el.value().attr("title") {
            Some(attr.to_string())
        } else {
            let t = el.text().collect::<String>();
            if t.trim().is_empty() {
                None
            } else {
                Some(t)
            }
        }
    });

    let pub_date = time_str.as_deref().and_then(parse_pub_date);

    let description = doc
        .select(&sel_content)
        .next()
        .map(|el| util::element_html(&el));

    Ok((pub_date, description))
}

fn extract_list(html: &str, limit: usize) -> Result<Vec<(String, String)>> {
    let doc = Html::parse_document(html);
    let sel_item = Selector::parse(".right-info .title a")
        .map_err(|e| Error::Parse(format!("studygolang: invalid list selector: {e}")))?;

    let mut items = Vec::new();
    for a in doc.select(&sel_item).take(limit) {
        let title = a.text().collect::<String>().trim().to_string();
        if title.is_empty() {
            continue;
        }
        let href = a.value().attr("href").unwrap_or("");
        if href.is_empty() {
            continue;
        }
        let link = util::absolutize(ROOT_URL, href);
        items.push((title, link));
    }
    Ok(items)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let id = ctx.param_str("id").unwrap_or("weekly");
    let limit = ctx.param_i64("limit").unwrap_or(20).max(1).min(50) as usize;

    let current_url = format!("{}/go/{}", ROOT_URL, id);
    let html = util::get_html(&current_url).await?;
    let list = extract_list(&html, limit)?;

    let mut items = Vec::new();
    for (title, link) in list {
        let (pub_date, description) = fetch_topic_detail(&link).await.unwrap_or((None, None));
        items.push(HubItem {
            title,
            description,
            link: Some(link),
            author: None,
            pub_date,
            categories: Vec::new(),
        });
    }

    Ok(HubData {
        title: format!("Go 语言中文网 - {}", id),
        description: Some(format!("Go 语言中文网 {} 板块主题列表。", id)),
        link: Some(current_url),
        image: None,
        language: None,
        items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_STUDYGOLANG_GO: Route = Route {
    meta: &META_STUDYGOLANG_GO,
    handler: handler_fn,
};
