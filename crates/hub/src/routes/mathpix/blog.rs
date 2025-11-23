use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset};
use scraper::{Html, Selector};

const BASE_URL: &str = "https://mathpix.com";

pub const META_MATHPIX_BLOG: RouteMeta = RouteMeta {
    hub_id: "mathpix/blog",
    path: "/mathpix/blog",
    categories: &["blog"],
    example: "/mathpix/blog",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["mathpix.com/blog"],
        target: "/blog",
    }],
    name: "Mathpix Blog",
    maintainers: &["captura"],
    url: "https://mathpix.com/blog",
    description: "Official Mathpix blog, a lightweight implementation aligned with RSSHub /mathpix/blog.",
    default_view: Some("articles"),
};

fn parse_date(raw: &str) -> Option<DateTime<FixedOffset>> {
    util::parse_date(raw)
}

fn extract_items(html: &str, limit: usize) -> Result<Vec<HubItem>> {
    let doc = Html::parse_document(html);
    let sel_item = Selector::parse("li.articles__item")
        .map_err(|e| Error::Parse(format!("mathpix: invalid item selector: {e}")))?;
    let sel_title = Selector::parse("a.articles__title")
        .map_err(|e| Error::Parse(format!("mathpix: invalid title selector: {e}")))?;
    let sel_image = Selector::parse("div.articles__image img")
        .map_err(|e| Error::Parse(format!("mathpix: invalid image selector: {e}")))?;
    let sel_text = Selector::parse("div.articles__text")
        .map_err(|e| Error::Parse(format!("mathpix: invalid text selector: {e}")))?;
    let sel_date = Selector::parse("time.articles__date")
        .map_err(|e| Error::Parse(format!("mathpix: invalid date selector: {e}")))?;

    let mut items = Vec::new();

    for li in doc.select(&sel_item).take(limit) {
        let title_el = li.select(&sel_title).next();
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
        let link = util::absolutize(BASE_URL, href);

        let img = li
            .select(&sel_image)
            .next()
            .and_then(|el| el.value().attr("srcset"))
            .or_else(|| {
                li.select(&sel_image)
                    .next()
                    .and_then(|el| el.value().attr("src"))
            })
            .map(|s| s.to_string());
        let img = img.map(|src| util::absolutize(BASE_URL, &src));

        let intro = li
            .select(&sel_text)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();

        let date_raw = li
            .select(&sel_date)
            .next()
            .and_then(|el| el.value().attr("datetime"))
            .map(|s| s.to_string());
        let pub_date = date_raw.as_deref().and_then(parse_date);

        let mut html_desc = String::new();
        if let Some(src) = &img {
            html_desc.push_str(&format!(
                "<p><img src=\"{src}\" alt=\"{alt}\"></p>",
                src = src,
                alt = title
            ));
        }
        if !intro.is_empty() {
            if !html_desc.is_empty() {
                html_desc.push_str("<p></p>");
            }
            html_desc.push_str(&format!("<p>{}</p>", intro));
        }

        items.push(HubItem {
            title,
            description: if html_desc.is_empty() {
                None
            } else {
                Some(html_desc)
            },
            link: Some(link),
            author: None,
            pub_date,
            categories: Vec::new(),
        });
    }

    Ok(items)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let limit = ctx.param_i64("limit").unwrap_or(30).max(1) as usize;
    let url = format!("{}/blog", BASE_URL);
    let html = util::get_html(&url).await?;
    let items = extract_items(&html, limit)?;

    let doc = Html::parse_document(&html);
    let sel_title = Selector::parse("title").unwrap();
    let sel_meta_desc = Selector::parse("meta[property=\"og:description\"]").unwrap();
    let sel_meta_img = Selector::parse("meta[property=\"og:image\"]").unwrap();

    let title = doc
        .select(&sel_title)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .unwrap_or_else(|| "Mathpix Blog".to_string());
    let description = doc
        .select(&sel_meta_desc)
        .next()
        .and_then(|el| el.value().attr("content"))
        .map(|s| s.to_string());
    let image = doc
        .select(&sel_meta_img)
        .next()
        .and_then(|el| el.value().attr("content"))
        .map(|s| s.to_string());
    let language = doc
        .select(&Selector::parse("html").unwrap())
        .next()
        .and_then(|el| el.value().attr("lang"))
        .map(|s| s.to_string());

    Ok(HubData {
        title,
        description,
        link: Some(url),
        image,
        language,
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_MATHPIX_BLOG: Route = Route {
    meta: &META_MATHPIX_BLOG,
    handler: handler_fn,
};
