use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_hub_macros::register_hub_route;

pub const META_V2EX_TAB: RouteMeta = RouteMeta {
    hub_id: "v2ex/tab",
    path: "/v2ex/tab/:tabid",
    categories: &["bbs"],
    example: "/v2ex/tab/hot",
    params: &[ParamMeta {
        name: "tabid",
        description: "tab id from V2EX URL, e.g. 'hot', 'tech'",
        default: Some("hot"),
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["www.v2ex.com", "v2ex.com"],
        target: "/?tab=:tabid",
    }],
    name: "V2EX Tab",
    maintainers: &["captura"],
    url: "https://www.v2ex.com/",
    description: "V2EX topics under a specific tab (HTML scraping, inspired by RSSHub v2ex/tab).",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let tabid = ctx.param_str("tabid").unwrap_or("hot");
    let host = "https://www.v2ex.com";
    let page_url = format!("{}/?tab={}", host, tabid);

    let html = util::get_html(&page_url).await?;

    let mut links: Vec<String> = Vec::new();
    let limit = ctx.param_i64("limit").unwrap_or(10) as usize;

    util::for_each_element(&html, "span.item_title > a", |el| {
        if links.len() >= limit {
            return;
        }
        if let Some(href) = el.value().attr("href") {
            let href_clean = href.split('#').next().unwrap_or("").to_string();
            if !href_clean.is_empty() {
                let abs = util::absolutize(host, &href_clean);
                links.push(abs);
            }
        }
    })?;

    let mut items = Vec::new();
    for link in links {
        let detail_html = match util::get_html(&link).await {
            Ok(h) => h,
            Err(_) => continue,
        };
        let doc = scraper::Html::parse_document(&detail_html);

        let title = scraper::Selector::parse(".header h1")
            .ok()
            .and_then(|sel| doc.select(&sel).next())
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_else(|| link.clone());

        let author = scraper::Selector::parse("div.header > small > a")
            .ok()
            .and_then(|sel| doc.select(&sel).next())
            .map(|el| el.text().collect::<String>().trim().to_string());

        let topic_html = scraper::Selector::parse("div.topic_content")
            .ok()
            .and_then(|sel| doc.select(&sel).next())
            .map(|el| util::element_html(&el))
            .unwrap_or_default();

        // Replies
        let mut reply_html = String::new();
        if let Ok(sel) = scraper::Selector::parse("[id^=\"r_\"]") {
            for node in doc.select(&sel) {
                let post = node;
                let content = scraper::Selector::parse(".reply_content")
                    .ok()
                    .and_then(|s| post.select(&s).next())
                    .map(|el| util::element_html(&el))
                    .unwrap_or_default();
                let reply_author = scraper::Selector::parse(".dark")
                    .ok()
                    .and_then(|s| post.select(&s).next())
                    .map(|el| el.text().collect::<String>().trim().to_string())
                    .unwrap_or_default();
                let no = scraper::Selector::parse(".no")
                    .ok()
                    .and_then(|s| post.select(&s).next())
                    .map(|el| el.text().collect::<String>().trim().to_string())
                    .unwrap_or_default();
                reply_html.push_str(&format!(
                    "<p><div>#{no}: <i>{author}</i></div><div>{content}</div></p>",
                    no = no,
                    author = reply_author,
                    content = content
                ));
            }
        }

        let description = format!("{}<div>{}</div>", topic_html, reply_html);

        items.push(HubItem {
            title,
            description: Some(description),
            link: Some(link.clone()),
            author,
            pub_date: None,
            categories: Vec::new(),
        });
    }

    Ok(HubData {
        title: format!("V2EX - tab {}", tabid),
        link: Some(page_url),
        description: Some(format!("V2EX tab {}", tabid)),
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
pub const ROUTE_V2EX_TAB: Route = Route {
    meta: &META_V2EX_TAB,
    handler: handler_fn,
};
