use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use scraper::{Html, Selector};

const ROOT_URL: &str = "https://www.linovelib.com";

pub const META_LINOVELIB_VOLUME: RouteMeta = RouteMeta {
    hub_id: "linovelib/volume",
    path: "/linovelib/volume/:id",
    categories: &["reading"],
    example: "/linovelib/volume/8",
    params: &[ParamMeta {
        name: "id",
        description: "Novel id from linovelib novel catalog URL.",
        default: None,
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["www.linovelib.com/novel/:id/catalog"],
        target: "/volume/:id",
    }],
    name: "哔哩轻小说 - 卷列表",
    maintainers: &["captura"],
    url: "https://www.linovelib.com",
    description: "Linovelib volume list, aligned with RSSHub /linovelib/volume/:id route.",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let id = ctx
        .param_str("id")
        .ok_or_else(|| Error::Config("linovelib/volume: id is required".to_string()))?;

    let current_url = format!("{ROOT_URL}/novel/{}/catalog", id);
    let html = util::get_html(&current_url).await?;

    let (page_title, volume_items) = {
        let doc = Html::parse_document(&html);

        let sel_title =
            Selector::parse(".book-meta h1").map_err(|e| Error::Parse(e.to_string()))?;
        let sel_volume = Selector::parse(".volume").map_err(|e| Error::Parse(e.to_string()))?;
        let sel_volume_title = Selector::parse("h2").map_err(|e| Error::Parse(e.to_string()))?;
        let sel_volume_cover =
            Selector::parse(".volume-cover").map_err(|e| Error::Parse(e.to_string()))?;

        let page_title = doc
            .select(&sel_title)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_else(|| format!("哔哩轻小说 {}", id));

        let mut volumes = Vec::new();
        for el in doc.select(&sel_volume) {
            let title = el
                .select(&sel_volume_title)
                .next()
                .map(|h| h.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            if title.is_empty() {
                continue;
            }
            let href = el
                .select(&sel_volume_cover)
                .next()
                .and_then(|a| a.value().attr("href"))
                .unwrap_or("")
                .trim();
            let link = if href.is_empty() {
                current_url.clone()
            } else if href.starts_with("http") {
                href.to_string()
            } else {
                format!("{ROOT_URL}{}", href)
            };

            volumes.push((title, link));
        }

        (page_title, volumes)
    };

    let mut items = Vec::new();
    // RSSHub uses reversed order for volumes.
    for (title, link) in volume_items.into_iter().rev() {
        items.push(HubItem {
            title,
            description: None,
            link: Some(link),
            author: None,
            pub_date: None,
            categories: Vec::new(),
        });
    }

    Ok(HubData {
        title: format!("{} - 哔哩轻小说", page_title),
        description: None,
        link: Some(current_url),
        image: None,
        language: Some("zh-CN".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_LINOVELIB_VOLUME: Route = Route {
    meta: &META_LINOVELIB_VOLUME,
    handler: handler_fn,
};
