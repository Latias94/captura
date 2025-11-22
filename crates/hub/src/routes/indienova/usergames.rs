use crate::routes::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use captura_hub_macros::register_hub_route;
use scraper::{Html, Selector};

use super::util::BASE_URL;

pub const META_INDIENOVA_USERGAMES: RouteMeta = RouteMeta {
    hub_id: "indienova/usergames",
    path: "/indienova/usergames",
    categories: &["game"],
    example: "/indienova/usergames",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["indienova.com/usergames", "indienova.com/"],
        target: "/usergames",
    }],
    name: "indienova - User games",
    maintainers: &["captura"],
    url: "https://indienova.com/usergames",
    description: "indienova user-developed games library.",
    default_view: Some("articles"),
};

pub async fn handler(_ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let link = format!("{}/usergames", BASE_URL);
    let html = crate::routes::util::get_html(&link).await?;
    let doc = Html::parse_document(&html);

    let sel = Selector::parse(".steam-game").unwrap();
    let mut items: Vec<HubItem> = Vec::new();

    for el in doc.select(&sel) {
        let title = el
            .value()
            .attr("title")
            .map(|s| s.to_string())
            .unwrap_or_default();
        if title.is_empty() {
            continue;
        }
        let a_sel = Selector::parse("a").unwrap();
        let a = match el.select(&a_sel).next() {
            Some(a) => a,
            None => continue,
        };
        let href = match a.value().attr("href") {
            Some(h) => h,
            None => continue,
        };
        let game_link = crate::routes::util::absolutize(&link, href);
        let author_sel = Selector::parse("span").unwrap();
        let author = el
            .select(&author_sel)
            .next()
            .map(|s| crate::routes::util::element_text(&s))
            .filter(|s| !s.is_empty());

        items.push(HubItem {
            title,
            description: None,
            link: Some(game_link),
            author,
            pub_date: None,
            categories: vec!["indienova".to_string(), "usergames".to_string()],
        });
    }

    Ok(HubData {
        title: "indienova user games".to_string(),
        description: None,
        link: Some(link),
        image: None,
        language: Some("zh-CN".to_string()),
        items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_INDIENOVA_USERGAMES: Route = Route {
    meta: &META_INDIENOVA_USERGAMES,
    handler: handler_fn,
};
