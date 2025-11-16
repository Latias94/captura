use crate::routes::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use crate::routes::util;
use captura_hub_macros::register_hub_route;

pub const META_LOBSTERS_FRONT: RouteMeta = RouteMeta {
    hub_id: "lobsters/front",
    path: "/lobsters/front",
    categories: &["community"],
    example: "/lobsters/front",
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
        source: &["lobste.rs"],
        target: "/",
    }],
    name: "Lobsters Front Page",
    maintainers: &["captura"],
    url: "https://lobste.rs/",
    description: "Lobsters front page stories.",
};

pub async fn handler(_ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let url = "https://lobste.rs/".to_string();

    let html = util::get_html(&url).await?;

    let mut items = Vec::new();
    util::for_each_element(&html, "li.story", |el| {
        let link = util::extract_attr(&el, "h2 a@href").map(|href| util::absolutize(&url, &href));
        let title = util::extract_text(&el, "h2 a");
        let desc_html = util::element_html(&el);
        items.push(HubItem {
            title: title.unwrap_or_else(|| link.clone().unwrap_or_default()),
            description: Some(desc_html),
            link,
            author: None,
            pub_date: None,
            categories: Vec::new(),
        });
    })?;

    Ok(HubData {
        title: "Lobsters Front Page".to_string(),
        description: Some("Lobsters front page stories.".to_string()),
        link: Some("https://lobste.rs/".to_string()),
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
pub const ROUTE_LOBSTERS_FRONT: Route = Route {
    meta: &META_LOBSTERS_FRONT,
    handler: handler_fn,
};
