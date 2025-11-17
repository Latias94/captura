use crate::routes::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use crate::routes::util;
use captura_hub_macros::register_hub_route;

pub const META_HN_FRONT: RouteMeta = RouteMeta {
    hub_id: "hn/front",
    path: "/hn/front",
    categories: &["community"],
    example: "/hn/front",
    params: &[],
    features: Features {
        require_config: &[],
        require_puppeteer: false,
        anti_crawler: false,
        support_bt: false,
        support_podcast: false,
        support_scihub: false,
        nsfw: false,
    },
    radar: &[Radar {
        source: &["news.ycombinator.com"],
        target: "/",
    }],
    name: "Hacker News Front Page",
    maintainers: &["captura"],
    url: "https://news.ycombinator.com/",
    description: "Hacker News front page stories.",
    default_view: Some("articles"),
};

pub async fn handler(_ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let url = "https://news.ycombinator.com/".to_string();

    let html = util::get_html(&url).await?;

    let mut items = Vec::new();
    util::for_each_element(&html, "tr.athing", |el| {
        let link = util::extract_attr(&el, "span.titleline a@href")
            .map(|href| util::absolutize(&url, &href));
        let title = util::extract_text(&el, "span.titleline a");
        let desc_html = util::element_html(&el);
        items.push(HubItem {
            title: title
                .clone()
                .unwrap_or_else(|| link.clone().unwrap_or_default()),
            description: Some(desc_html),
            link,
            author: None,
            pub_date: None,
            categories: Vec::new(),
        });
    })?;

    Ok(HubData {
        title: "Hacker News Front Page".to_string(),
        description: Some("Hacker News front page stories.".to_string()),
        link: Some("https://news.ycombinator.com/".to_string()),
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
pub const ROUTE_HN_FRONT: Route = Route {
    meta: &META_HN_FRONT,
    handler: handler_fn,
};
