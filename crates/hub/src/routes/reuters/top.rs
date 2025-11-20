use crate::routes::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use crate::routes::util;
use captura_hub_macros::register_hub_route;

pub const META_REUTERS_TOP: RouteMeta = RouteMeta {
    hub_id: "reuters/top",
    path: "/reuters/top",
    categories: &["news"],
    example: "/reuters/top",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["www.reuters.com"],
        target: "/world/",
    }],
    name: "Reuters Top News",
    maintainers: &["captura"],
    url: "https://www.reuters.com/world/",
    description: "Reuters top news stories.",
    default_view: Some("articles"),
};

pub async fn handler(_ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let url = "https://www.reuters.com/world/".to_string();

    let html = util::get_html(&url).await?;

    let mut items = Vec::new();
    util::for_each_element(&html, "article.story-card, article.story", |el| {
        let link = util::extract_attr(&el, "a@href").map(|href| util::absolutize(&url, &href));
        let title = util::extract_text(&el, "h3").or_else(|| util::extract_text(&el, "h2"));
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
        title: "Reuters Top News".to_string(),
        description: Some("Reuters top news stories.".to_string()),
        link: Some("https://www.reuters.com/world/".to_string()),
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
pub const ROUTE_REUTERS_TOP: Route = Route {
    meta: &META_REUTERS_TOP,
    handler: handler_fn,
};
