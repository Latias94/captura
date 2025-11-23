use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeZone};
use regex::Regex;
use scraper::{Html, Selector};

const ROOT_URL: &str = "https://www.aisixiang.com";

pub const META_AISIXIANG_ZHUANTI: RouteMeta = RouteMeta {
    hub_id: "aisixiang/zhuanti",
    path: "/aisixiang/zhuanti/:id",
    categories: &["reading"],
    example: "/aisixiang/zhuanti/211",
    params: &[ParamMeta {
        name: "id",
        description: "Topic id from Aisixiang zhuanti URLs, e.g. 211.",
        default: None,
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["www.aisixiang.com/zhuanti/:id.html"],
        target: "/zhuanti/:id",
    }],
    name: "爱思想专题",
    maintainers: &["captura"],
    url: "https://www.aisixiang.com",
    description: "Aisixiang topic articles list with full content, aligned with RSSHub /aisixiang/zhuanti/:id.",
    default_view: Some("articles"),
};

fn parse_cn_datetime_local(s: &str) -> Option<DateTime<FixedOffset>> {
    let re = Regex::new(r"更新时间：\s*(\d{4}-\d{2}-\d{2}\s+\d{2}:\d{2})").ok()?;
    let caps = re.captures(s)?;
    let dt_str = caps.get(1)?.as_str();
    let naive = NaiveDateTime::parse_from_str(dt_str, "%Y-%m-%d %H:%M").ok()?;
    let offset = FixedOffset::east_opt(8 * 3600)?;
    Some(offset.from_local_datetime(&naive).single()?)
}

fn extract_list(html: &str, limit: usize) -> captura_common::Result<Vec<(String, String)>> {
    let doc = Html::parse_document(html);
    let sel_item = Selector::parse("div.article-title")
        .map_err(|e| Error::Parse(format!("aisixiang/zhuanti: list selector error: {e}")))?;
    let sel_a = Selector::parse("a")
        .map_err(|e| Error::Parse(format!("aisixiang/zhuanti: link selector error: {e}")))?;

    let mut out = Vec::new();
    for title_div in doc.select(&sel_item).take(limit) {
        let a = match title_div
            .select(&sel_a)
            .find(|a| a.value().attr("href").is_some())
        {
            Some(a) => a,
            None => continue,
        };

        let href = match a.value().attr("href") {
            Some(h) if !h.trim().is_empty() => h.trim(),
            _ => continue,
        };
        let link = util::absolutize(ROOT_URL, href);

        let title = a.text().collect::<String>().trim().to_string();
        if title.is_empty() {
            continue;
        }

        out.push((title, link));
    }

    Ok(out)
}

fn extract_detail(
    html: &str,
) -> captura_common::Result<(
    Option<String>,
    Option<String>,
    Option<DateTime<FixedOffset>>,
    Vec<String>,
)> {
    let doc = Html::parse_document(html);

    let sel_title = Selector::parse("div.show_text h3").map_err(|e| Error::Parse(e.to_string()))?;
    let sel_info = Selector::parse("div.info").map_err(|e| Error::Parse(e.to_string()))?;
    let sel_about = Selector::parse("div.about strong").map_err(|e| Error::Parse(e.to_string()))?;
    let sel_article =
        Selector::parse("div.article-content").map_err(|e| Error::Parse(e.to_string()))?;

    let title = doc
        .select(&sel_title)
        .next()
        .map(|h| h.text().collect::<String>().trim().to_string())
        .filter(|s| !s.is_empty());

    let info_text = doc
        .select(&sel_info)
        .next()
        .map(|div| div.text().collect::<String>().trim().to_string())
        .unwrap_or_default();
    let pub_date = parse_cn_datetime_local(&info_text)
        .or_else(|| crate::routes::util::parse_cn_datetime(&info_text));

    let mut authors = Vec::new();
    for s in doc.select(&sel_about) {
        let t = s.text().collect::<String>().trim().to_string();
        if !t.is_empty() {
            authors.push(t);
        }
    }
    let author = if authors.is_empty() {
        None
    } else {
        Some(authors.join(", "))
    };

    let description = doc
        .select(&sel_article)
        .next()
        .map(|div| crate::routes::util::element_html(&div))
        .filter(|s| !s.trim().is_empty());

    let mut categories = Vec::new();
    if let Ok(sel_u) = Selector::parse("u") {
        for u in doc.select(&sel_u) {
            let t = u.text().collect::<String>().trim().to_string();
            if !t.is_empty() {
                categories.push(t);
            }
        }
    }

    let _ = author;

    Ok((title, description, pub_date, categories))
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let id = ctx
        .param_str("id")
        .ok_or_else(|| Error::Config("aisixiang/zhuanti: missing id".to_string()))?;
    let limit = ctx.param_i64("limit").unwrap_or(30).max(1) as usize;

    let url = format!("{}/zhuanti/{}.html", ROOT_URL, id);
    let html = util::get_html(&url).await?;
    let list = extract_list(&html, limit)?;

    let mut items = Vec::new();
    for (fallback_title, link) in list {
        let detail_html = match util::get_html(&link).await {
            Ok(h) => h,
            Err(_) => {
                items.push(HubItem {
                    title: fallback_title.clone(),
                    description: None,
                    link: Some(link.clone()),
                    author: None,
                    pub_date: None,
                    categories: Vec::new(),
                });
                continue;
            }
        };

        let (title_opt, desc_opt, pub_date, categories) =
            extract_detail(&detail_html).unwrap_or((None, None, None, Vec::new()));
        let title = title_opt.unwrap_or_else(|| fallback_title.clone());

        items.push(HubItem {
            title,
            description: desc_opt,
            link: Some(link.clone()),
            author: None,
            pub_date,
            categories,
        });
    }

    let doc = Html::parse_document(&html);
    let tips_title = doc
        .select(&Selector::parse("div.tips h2").unwrap())
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .unwrap_or_else(|| format!("专题 {}", id));
    let desc = doc
        .select(&Selector::parse("div.tips p").unwrap())
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string());

    Ok(HubData {
        title: format!("爱思想 - {}", tips_title),
        description: desc,
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
pub const ROUTE_AISIXIANG_ZHUANTI: Route = Route {
    meta: &META_AISIXIANG_ZHUANTI,
    handler: handler_fn,
};
