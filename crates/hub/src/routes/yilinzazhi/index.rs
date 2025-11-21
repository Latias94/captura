use crate::routes::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use scraper::{Html, Selector};

const ROOT_URL: &str = "https://www.yilinzazhi.com";

pub const META_YILIN_INDEX: RouteMeta = RouteMeta {
    hub_id: "yilinzazhi/index",
    path: "/yilinzazhi",
    categories: &["reading"],
    example: "/yilinzazhi",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["www.yilinzazhi.com"],
        target: "/",
    }],
    name: "意林文章列表",
    maintainers: &["captura"],
    url: "https://www.yilinzazhi.com",
    description:
        "Yilin magazine site front-page article lists, aligned with RSSHub /yilinzazhi route.",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let limit = ctx.param_i64("limit").unwrap_or(20).max(1) as usize;
    let url = format!("{}/", ROOT_URL);
    let html = util::get_html(&url).await?;
    let (items, _) = {
        let doc = Html::parse_document(&html);

        let sel_item = Selector::parse("section.content li")
            .map_err(|e| Error::Parse(format!("yilinzazhi: list selector error: {e}")))?;
        let sel_a = Selector::parse("a")
            .map_err(|e| Error::Parse(format!("yilinzazhi: link selector error: {e}")))?;

        let mut links = Vec::new();

        for li in doc.select(&sel_item).take(limit) {
            let a = match li.select(&sel_a).next() {
                Some(a) => a,
                None => continue,
            };
            let title = a.text().collect::<String>().trim().to_string();
            if title.is_empty() {
                continue;
            }
            let href = a.value().attr("href").unwrap_or("").trim();
            if href.is_empty() {
                continue;
            }
            let link = util::absolutize(&url, href);
            links.push((title, link));
        }
        (links, ())
    };

    let mut items_out = Vec::new();

    for (title, link) in items {
        let detail_html = match util::get_html(&link).await {
            Ok(h) => h,
            Err(_) => {
                items_out.push(HubItem {
                    title,
                    description: None,
                    link: Some(link),
                    author: None,
                    pub_date: None,
                    categories: Vec::new(),
                });
                continue;
            }
        };
        let detail = Html::parse_document(&detail_html);
        let sel_box = Selector::parse("div.maglistbox")
            .map_err(|e| Error::Parse(format!("yilinzazhi: maglistbox selector error: {e}")))?;
        let description = detail
            .select(&sel_box)
            .next()
            .map(|el| el.html())
            .filter(|s| !s.trim().is_empty());

        items_out.push(HubItem {
            title,
            description,
            link: Some(link),
            author: None,
            pub_date: None,
            categories: Vec::new(),
        });
    }

    Ok(HubData {
        title: "意林杂志网".to_string(),
        description: None,
        link: Some(url),
        image: None,
        language: Some("zh-CN".to_string()),
        items: items_out,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_YILIN_INDEX: Route = Route {
    meta: &META_YILIN_INDEX,
    handler: handler_fn,
};
