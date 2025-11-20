use crate::routes::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use crate::routes::util;
use captura_hub_macros::register_hub_route;

pub const META_V2EX_XNA: RouteMeta = RouteMeta {
    hub_id: "v2ex/xna",
    path: "/v2ex/xna",
    categories: &["bbs", "blog"],
    example: "/v2ex/xna",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["www.v2ex.com", "v2ex.com"],
        target: "/xna",
    }],
    name: "V2EX XNA",
    maintainers: &["captura"],
    url: "https://www.v2ex.com/xna",
    description: "V2EX XNA entries (external blogs/links), inspired by RSSHub v2ex/xna.",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let host = "https://www.v2ex.com";
    let page_url = format!("{}/xna", host);

    let html = util::get_html(&page_url).await?;

    let limit = ctx.param_i64("limit").unwrap_or(50) as usize;
    let mut items = Vec::new();

    let doc = scraper::Html::parse_document(&html);
    if let Ok(sel) = scraper::Selector::parse("div.xna-entry-main-container") {
        for node in doc.select(&sel).take(limit) {
            let link_sel = scraper::Selector::parse(".xna-entry-title > a");
            let author_sel = scraper::Selector::parse(".xna-source-author > a");

            let (title, link) = if let Ok(ls) = &link_sel {
                if let Some(a) = node.select(ls).next() {
                    let title = a.text().collect::<String>().trim().to_string();
                    let href = a.value().attr("href").unwrap_or("").to_string();
                    let abs = util::absolutize(host, &href);
                    (title, Some(abs))
                } else {
                    (String::new(), None)
                }
            } else {
                (String::new(), None)
            };

            if title.is_empty() {
                continue;
            }

            let author = if let Ok(sel) = &author_sel {
                node.select(sel)
                    .next()
                    .map(|a| a.text().collect::<String>().trim().to_string())
            } else {
                None
            };

            items.push(HubItem {
                title: title.clone(),
                description: Some(title.clone()),
                link,
                author,
                pub_date: None,
                categories: Vec::new(),
            });
        }
    }

    Ok(HubData {
        title: "V2EX-XNA".to_string(),
        link: Some(page_url),
        description: Some("V2EX XNA entries".to_string()),
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
pub const ROUTE_V2EX_XNA: Route = Route {
    meta: &META_V2EX_XNA,
    handler: handler_fn,
};
