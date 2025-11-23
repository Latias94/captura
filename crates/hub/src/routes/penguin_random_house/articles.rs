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

pub const META_PRH_ARTICLES: RouteMeta = RouteMeta {
    hub_id: "penguin-random-house/articles",
    path: "/penguin-random-house/articles",
    categories: &["reading"],
    example: "/penguin-random-house/articles",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["penguinrandomhouse.com/articles"],
        target: "/articles",
    }],
    name: "Penguin Random House Articles",
    maintainers: &["captura"],
    url: "https://www.penguinrandomhouse.com/articles",
    description: "In-depth interviews and essays from Penguin Random House, aligned with RSSHub /penguin-random-house/articles route.",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let limit = ctx.param_i64("limit").unwrap_or(20).max(1) as usize;
    let link = format!("{}/articles/", ROOT_URL);
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

    let sel_header = Selector::parse("h2.hdr-smalltxt")
        .map_err(|e| Error::Parse(format!("prh: header selector error: {e}")))?;
    let sel_img = Selector::parse("div.img-block > img")
        .map_err(|e| Error::Parse(format!("prh: image selector error: {e}")))?;
    let sel_content = Selector::parse("div.main-content > p, div.main-content > ul")
        .map_err(|e| Error::Parse(format!("prh: main-content selector error: {e}")))?;

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

        let mut header_html = String::new();
        if let Some(h) = detail.select(&sel_header).next() {
            header_html.push_str(&h.html());
        }
        if let Some(img) = detail.select(&sel_img).next() {
            header_html.push_str(&img.html());
        }

        let mut body_html = String::new();
        for el in detail.select(&sel_content) {
            body_html.push_str(&el.html());
        }

        let mut description = String::new();
        if !header_html.is_empty() {
            description.push_str(&header_html);
            description.push_str("<br>");
        }
        description.push_str(&body_html);

        let pub_date = parse_pub_date(&detail);

        items.push(HubItem {
            title,
            description: if description.trim().is_empty() {
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
        title: "Penguin Random House Articles".to_string(),
        description: Some(
            "In-depth interviews, essays and reading guides from Penguin Random House.".to_string(),
        ),
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
pub const ROUTE_PRH_ARTICLES: Route = Route {
    meta: &META_PRH_ARTICLES,
    handler: handler_fn,
};
