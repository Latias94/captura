use crate::routes::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use captura_hub_macros::register_hub_route;
use scraper::{Html, Selector};

const BASE_URL: &str = "https://www.playstation.com/en-sg/ps-plus/whats-new/";

pub const META_PS_MONTHLY_GAMES: RouteMeta = RouteMeta {
    hub_id: "ps/monthly-games",
    path: "/ps/monthly-games",
    categories: &["game"],
    example: "/ps/monthly-games",
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
        source: &["www.playstation.com/en-sg/ps-plus/whats-new"],
        target: "/monthly-games",
    }],
    name: "PlayStation Monthly Games",
    maintainers: &["captura"],
    url: "https://www.playstation.com/en-sg/ps-plus/whats-new",
    description: "PlayStation Plus monthly games.",
    default_view: Some("notifications"),
};

pub async fn handler(_ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let html = crate::routes::util::get_html(BASE_URL).await?;
    let doc = Html::parse_document(&html);

    let box_sel = Selector::parse("#monthly-games .box--light").unwrap();
    let title_sel = Selector::parse("h3").unwrap();
    let text_sel = Selector::parse("h3 + p").unwrap();
    let img_sel = Selector::parse(".media-block__img source").unwrap();
    let link_sel = Selector::parse(".btn--cta").unwrap();

    let mut items = Vec::new();

    for box_el in doc.select(&box_sel) {
        let title = box_el
            .select(&title_sel)
            .next()
            .map(|h| crate::routes::util::element_text(&h))
            .unwrap_or_default();
        if title.is_empty() {
            continue;
        }

        let img = box_el
            .select(&img_sel)
            .next()
            .and_then(|s| s.value().attr("srcset"))
            .map(|s| s.to_string());
        let text = box_el
            .select(&text_sel)
            .next()
            .map(|p| crate::routes::util::element_text(&p))
            .filter(|s| !s.is_empty());
        let link = box_el
            .select(&link_sel)
            .next()
            .and_then(|a| a.value().attr("href"))
            .map(|href| crate::routes::util::absolutize(BASE_URL, href));

        let mut description = String::new();
        if let Some(ref img) = img {
            description.push_str("<p>");
            description.push_str(&crate::routes::util::html_img(img, &title));
            description.push_str("</p>");
        }
        if let Some(ref t) = text {
            description.push_str("<p>");
            description.push_str(t);
            description.push_str("</p>");
        }

        items.push(HubItem {
            title,
            description: if description.is_empty() {
                None
            } else {
                Some(description)
            },
            link,
            author: None,
            pub_date: None,
            categories: vec!["ps".to_string(), "monthly-games".to_string()],
        });
    }

    Ok(HubData {
        title: "PlayStation Plus Monthly Games".to_string(),
        description: None,
        link: Some(BASE_URL.to_string()),
        image: None,
        language: Some("en".to_string()),
        items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_PS_MONTHLY_GAMES: Route = Route {
    meta: &META_PS_MONTHLY_GAMES,
    handler: handler_fn,
};
