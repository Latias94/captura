use crate::routes::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use crate::routes::util;
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use scraper::{Html, Selector};

const ROOT_URL: &str = "https://www.joshwcomeau.com";

pub const META_JOSHWCOMEAU_POPULAR: RouteMeta = RouteMeta {
    hub_id: "joshwcomeau/popular",
    path: "/joshwcomeau/popular",
    categories: &["programming"],
    example: "/joshwcomeau/popular",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["www.joshwcomeau.com/"],
        target: "/popular",
    }],
    name: "Josh W. Comeau Popular Content",
    maintainers: &["captura"],
    url: "https://www.joshwcomeau.com",
    description: "Popular content from Josh W. Comeau's blog, aligned with RSSHub /joshwcomeau/popular route.",
    default_view: Some("articles"),
};

fn extract_links(html: &str, limit: usize) -> Result<Vec<(String, String)>> {
    let doc = Html::parse_document(html);
    let sel_section = Selector::parse(r#"section[style*="grid-area:popular"]"#)
        .map_err(|e| Error::Parse(format!("joshwcomeau: invalid section selector: {e}")))?;
    let section = doc
        .select(&sel_section)
        .next()
        .ok_or_else(|| Error::Parse("joshwcomeau: popular section not found".to_string()))?;

    let sel_link = Selector::parse("ol li a")
        .map_err(|e| Error::Parse(format!("joshwcomeau: invalid link selector: {e}")))?;

    let mut out = Vec::new();
    for a in section.select(&sel_link).take(limit) {
        let href = a.value().attr("href").unwrap_or("");
        if href.is_empty() {
            continue;
        }
        let title = a.text().collect::<String>().trim().to_string();
        if title.is_empty() {
            continue;
        }
        out.push((href.to_string(), title));
    }

    Ok(out)
}

fn extract_meta_content(doc: &Html, selector: &str, attr: &str) -> Option<String> {
    let sel = Selector::parse(selector).ok()?;
    doc.select(&sel)
        .next()
        .and_then(|el| el.value().attr(attr))
        .map(|s| s.to_string())
}

async fn fetch_post(url: &str, fallback_title: &str) -> Result<HubItem> {
    let html = util::get_html(url).await?;
    let doc = Html::parse_document(&html);

    let full_title =
        extract_meta_content(&doc, r#"meta[property="og:title"]"#, "content").unwrap_or_default();
    let mut title = if full_title.is_empty() {
        fallback_title.to_string()
    } else {
        full_title
            .replace('•', "•")
            .replace("• Josh W. Comeau", "")
            .trim()
            .to_string()
    };
    if title.is_empty() {
        title = fallback_title.to_string();
    }

    let summary = extract_meta_content(&doc, r#"meta[property="og:description"]"#, "content");
    let author = extract_meta_content(&doc, r#"meta[name="author"]"#, "content")
        .filter(|s| !s.trim().is_empty());

    let sel_main = Selector::parse("main").unwrap();
    let description = doc
        .select(&sel_main)
        .next()
        .map(|el| util::element_html(&el))
        .or(summary);

    Ok(HubItem {
        title,
        description,
        link: Some(url.to_string()),
        author,
        pub_date: None,
        categories: Vec::new(),
    })
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let limit = ctx.param_i64("limit").unwrap_or(10).max(1) as usize;
    let html = util::get_html(ROOT_URL).await?;
    let links = extract_links(&html, limit)?;

    let mut items = Vec::new();
    for (href, card_title) in links {
        if href.starts_with("http://") || href.starts_with("https://") {
            items.push(HubItem {
                title: card_title.clone(),
                description: Some("Read it on the external site.".to_string()),
                link: Some(href.clone()),
                author: None,
                pub_date: None,
                categories: Vec::new(),
            });
            continue;
        }

        let url = format!("{}{}", ROOT_URL, href);
        match fetch_post(&url, &card_title).await {
            Ok(item) => items.push(item),
            Err(_) => items.push(HubItem {
                title: card_title.clone(),
                description: None,
                link: Some(url),
                author: None,
                pub_date: None,
                categories: Vec::new(),
            }),
        }
    }

    Ok(HubData {
        title: "Popular Content | Josh W. Comeau".to_string(),
        description: Some(
            "Friendly tutorials for developers. Focus on React, CSS, animation, and more!"
                .to_string(),
        ),
        link: Some(ROOT_URL.to_string()),
        image: None,
        language: Some("en".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_JOSHWCOMEAU_POPULAR: Route = Route {
    meta: &META_JOSHWCOMEAU_POPULAR,
    handler: handler_fn,
};
