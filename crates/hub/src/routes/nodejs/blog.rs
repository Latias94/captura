use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;
use chrono::{DateTime, FixedOffset};
use scraper::{Html, Selector};

const ROOT_URL: &str = "https://nodejs.org";

pub const META_NODEJS_BLOG: RouteMeta = RouteMeta {
    hub_id: "nodejs/blog",
    path: "/nodejs/blog/:language?",
    categories: &["programming"],
    example: "/nodejs/blog",
    params: &[ParamMeta {
        name: "language",
        description: "语言代码，默认 en，可选 ar, ca, de, es, fa, fr, gl, it, ja, ko, pt-br, ro, ru, tr, uk, zh-cn, zh-tw 等。",
        default: Some("en"),
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["nodejs.org/:language/blog", "nodejs.org"],
        target: "/blog/:language?",
    }],
    name: "Node.js News",
    maintainers: &["captura"],
    url: "https://nodejs.org/en/blog",
    description: "Official Node.js blog news, aligned with RSSHub /nodejs/blog/:language route.",
    default_view: Some("articles"),
};

fn parse_pub_date(raw: &str) -> Option<DateTime<FixedOffset>> {
    util::parse_date(raw)
}

async fn fetch_list(language: &str) -> Result<String> {
    let current_url = format!("{}/{}/blog", ROOT_URL, language);
    let client = client_basic(None, None).map_err(|e| Error::Network(e.to_string()))?;
    let resp = client
        .get(&current_url)
        .send()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "{} -> http status {}",
            current_url, status
        )));
    }
    resp.text().await.map_err(|e| Error::Network(e.to_string()))
}

fn extract_items(html: &str, limit: usize) -> Result<Vec<HubItem>> {
    let doc = Html::parse_document(html);
    let sel_article = Selector::parse("article")
        .map_err(|e| Error::Parse(format!("nodejs: invalid article: {e}")))?;
    let sel_footer_p = Selector::parse("footer p")
        .map_err(|e| Error::Parse(format!("nodejs: invalid footer: {e}")))?;
    let sel_time = Selector::parse("footer time")
        .map_err(|e| Error::Parse(format!("nodejs: invalid time: {e}")))?;
    let sel_link = Selector::parse("a[aria-label]")
        .map_err(|e| Error::Parse(format!("nodejs: invalid link: {e}")))?;

    let mut items = Vec::new();
    for article in doc.select(&sel_article).take(limit) {
        let author = article
            .select(&sel_footer_p)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string());
        let pub_date = article
            .select(&sel_time)
            .next()
            .and_then(|el| el.value().attr("datetime"))
            .and_then(parse_pub_date);
        let link_el = article.select(&sel_link).next();
        let Some(link_el) = link_el else {
            continue;
        };
        let title = link_el.value().attr("aria-label").unwrap_or("").to_string();
        if title.trim().is_empty() {
            continue;
        }
        let href = link_el.value().attr("href").unwrap_or("");
        let link = if href.is_empty() {
            None
        } else {
            Some(util::absolutize(ROOT_URL, href))
        };

        items.push(HubItem {
            title,
            description: None,
            link,
            author,
            pub_date,
            categories: Vec::new(),
        });
    }
    Ok(items)
}

async fn enrich_item(mut item: HubItem) -> Result<HubItem> {
    if let Some(link) = &item.link {
        if let Ok(html) = util::get_html(link).await {
            let doc = Html::parse_document(&html);
            let sel_main = Selector::parse("main")
                .map_err(|e| Error::Parse(format!("nodejs: invalid main: {e}")))?;
            if let Some(main) = doc.select(&sel_main).next() {
                let body = util::element_html(&main);
                if !body.trim().is_empty() {
                    item.description = Some(body);
                }
            }
        }
    }
    Ok(item)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let language = ctx.param_str("language").unwrap_or("en");
    let html = fetch_list(language).await?;
    let mut items = extract_items(&html, ctx.param_i64("limit").unwrap_or(50).max(1) as usize)?;

    let mut enriched = Vec::new();
    for item in items {
        match enrich_item(item).await {
            Ok(it) => enriched.push(it),
            Err(_) => {}
        }
    }

    let title = "News - Node.js".to_string();
    let link = format!("{}/{}/blog", ROOT_URL, language);

    Ok(HubData {
        title,
        description: Some(format!(
            "Official Node.js blog news (language: {}).",
            language
        )),
        link: Some(link),
        image: None,
        language: None,
        items: enriched,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_NODEJS_BLOG: Route = Route {
    meta: &META_NODEJS_BLOG,
    handler: handler_fn,
};
