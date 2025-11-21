use crate::routes::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use crate::routes::util;
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset};
use scraper::{Html, Selector};

const BASE_URL: &str = "https://augmentcode.com";

pub const META_AUGMENTCODE_BLOG: RouteMeta = RouteMeta {
    hub_id: "augmentcode/blog",
    path: "/augmentcode/blog",
    categories: &["programming"],
    example: "/augmentcode/blog",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["augmentcode.com/blog"],
        target: "/blog",
    }],
    name: "AugmentCode Blog",
    maintainers: &["captura"],
    url: "https://augmentcode.com/blog",
    description: "Official AugmentCode blog, a lightweight implementation aligned with RSSHub /augmentcode/blog.",
    default_view: Some("articles"),
};

fn parse_pub_date(raw: &str) -> Option<DateTime<FixedOffset>> {
    util::parse_date(raw)
}

fn extract_list(html: &str, limit: usize) -> Result<Vec<HubItem>> {
    let doc = Html::parse_document(html);
    // 卡片在 <a href="..."><div data-slot="card">...</div></a> 结构内
    let sel_a = Selector::parse("a[href] > div[data-slot=\"card\"]")
        .map_err(|e| Error::Parse(format!("augmentcode: invalid card selector: {e}")))?;
    let sel_header = Selector::parse("div[data-slot=\"card-header\"] img")
        .map_err(|e| Error::Parse(format!("augmentcode: invalid header selector: {e}")))?;
    let sel_content = Selector::parse("div[data-slot=\"card-content\"]")
        .map_err(|e| Error::Parse(format!("augmentcode: invalid content selector: {e}")))?;
    let sel_footer = Selector::parse("div[data-slot=\"card-footer\"] p")
        .map_err(|e| Error::Parse(format!("augmentcode: invalid footer selector: {e}")))?;

    let mut items = Vec::new();

    for card in doc.select(&sel_a).take(limit) {
        let parent = match card.parent() {
            Some(node) => node,
            None => continue,
        };
        let parent_el = match scraper::ElementRef::wrap(parent) {
            Some(el) => el,
            None => continue,
        };
        let href = parent_el.value().attr("href").unwrap_or("");
        if href.is_empty() {
            continue;
        }
        let link = Some(util::absolutize(BASE_URL, href));

        let title = card
            .select(&sel_content)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();
        if title.is_empty() {
            continue;
        }

        let image = card
            .select(&sel_header)
            .next()
            .and_then(|el| el.value().attr("src"))
            .map(|s| s.to_string());

        let mut pub_date = None;
        let mut description = None;

        let footers: Vec<_> = card
            .select(&sel_footer)
            .map(|el| el.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if let Some(last) = footers.last() {
            pub_date = parse_pub_date(last);
        }

        if let Some(img) = &image {
            description = Some(format!(
                "<p><img src=\"{src}\" alt=\"{alt}\"></p>",
                src = img,
                alt = title
            ));
        }

        items.push(HubItem {
            title,
            description,
            link,
            author: None,
            pub_date,
            categories: Vec::new(),
        });
    }

    Ok(items)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let limit = ctx.param_i64("limit").unwrap_or(50).max(1) as usize;
    let url = format!("{}/blog", BASE_URL);
    let html = util::get_html(&url).await?;
    let items = extract_list(&html, limit)?;

    Ok(HubData {
        title: "AugmentCode Blog".to_string(),
        description: Some("Official posts from the AugmentCode blog.".to_string()),
        link: Some(url),
        image: None,
        language: None,
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_AUGMENTCODE_BLOG: Route = Route {
    meta: &META_AUGMENTCODE_BLOG,
    handler: handler_fn,
};
