use crate::routes::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset};
use scraper::{Html, Selector};

const ROOT_URL: &str = "https://www.penguinrandomhouse.com";

fn parse_pub_date(doc: &Html) -> Option<DateTime<FixedOffset>> {
    if let Ok(sel) = Selector::parse(r#"meta[property="article:published_time"]"#) {
        if let Some(meta) = doc.select(&sel).next() {
            if let Some(content) = meta.value().attr("content") {
                return crate::routes::util::parse_date(content);
            }
        }
    }
    None
}

pub const META_PRH_BOOK_LISTS: RouteMeta = RouteMeta {
    hub_id: "penguin-random-house/the-read-down",
    path: "/penguin-random-house/the-read-down",
    categories: &["reading"],
    example: "/penguin-random-house/the-read-down",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["penguinrandomhouse.com/the-read-down"],
        target: "/the-read-down",
    }],
    name: "Penguin Random House Book Lists",
    maintainers: &["captura"],
    url: "https://www.penguinrandomhouse.com/the-read-down",
    description:
        "Book list articles from Penguin Random House The Read Down, aligned with RSSHub /penguin-random-house/the-read-down route.",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let limit = ctx.param_i64("limit").unwrap_or(20).max(1) as usize;
    let link = format!("{}/the-read-down/", ROOT_URL);
    let html = util::get_html(&link).await?;
    let targets = {
        let doc = Html::parse_document(&html);

        let sel_module =
            Selector::parse(".archive-module-half-container, .archive-module-third-container")
                .map_err(|e| Error::Parse(format!("prh: module selector error: {e}")))?;
        let sel_text = Selector::parse(".archive-module-text")
            .map_err(|e| Error::Parse(format!("prh: text selector error: {e}")))?;
        let sel_link = Selector::parse("a")
            .map_err(|e| Error::Parse(format!("prh: link selector error: {e}")))?;

        let mut targets = Vec::new();

        for module in doc.select(&sel_module).take(limit) {
            let title = module
                .select(&sel_text)
                .next()
                .map(|el| el.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            if title.is_empty() {
                continue;
            }
            let a = match module.select(&sel_link).next() {
                Some(a) => a,
                None => continue,
            };
            let href = a.value().attr("href").unwrap_or("").trim();
            if href.is_empty() {
                continue;
            }
            let item_link = util::absolutize(ROOT_URL, href);
            targets.push((title, item_link));
        }

        targets
    };

    let mut items = Vec::new();

    let sel_header = Selector::parse("h2.read-down-text")
        .map_err(|e| Error::Parse(format!("prh: header selector error: {e}")))?;
    let sel_list_item = Selector::parse(".awesome-list > li")
        .map_err(|e| Error::Parse(format!("prh: list selector error: {e}")))?;

    for (title, item_link) in targets {
        let detail_html = match util::get_html(&item_link).await {
            Ok(h) => h,
            Err(_) => {
                items.push(HubItem {
                    title,
                    description: None,
                    link: Some(item_link),
                    author: None,
                    pub_date: None,
                    categories: Vec::new(),
                });
                continue;
            }
        };
        let detail = Html::parse_document(&detail_html);

        let mut description = String::new();
        if let Some(h) = detail.select(&sel_header).next() {
            description.push_str(&h.html());
        }
        let mut main_block = String::new();
        for li in detail.select(&sel_list_item) {
            main_block.push_str(&li.html());
        }
        if !main_block.is_empty() {
            if !description.is_empty() {
                description.push_str("<br>");
            }
            description.push_str(&main_block);
        }

        let pub_date = parse_pub_date(&detail);

        items.push(HubItem {
            title,
            description: if description.is_empty() {
                None
            } else {
                Some(description)
            },
            link: Some(item_link),
            author: None,
            pub_date,
            categories: Vec::new(),
        });
    }

    Ok(HubData {
        title: "Penguin Random House Book Lists".to_string(),
        description: Some("Never wonder what to read next - curated PRH book lists.".to_string()),
        link: Some(link),
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
pub const ROUTE_PRH_BOOK_LISTS: Route = Route {
    meta: &META_PRH_BOOK_LISTS,
    handler: handler_fn,
};
