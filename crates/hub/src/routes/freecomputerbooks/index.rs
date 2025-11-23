use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use scraper::{Html, Selector};

const ROOT_URL: &str = "https://freecomputerbooks.com";

pub const META_FREECOMPUTERBOOKS_INDEX: RouteMeta = RouteMeta {
    hub_id: "freecomputerbooks/index",
    path: "/freecomputerbooks/:category?",
    categories: &["reading"],
    example: "/freecomputerbooks/compscAlgorithmBooks",
    params: &[ParamMeta {
        name: "category",
        description: "Category id, corresponding to the HTML file name (without .html suffix) in a book list URL path.",
        default: None,
        options: &[],
    }],
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
        source: &[
            "freecomputerbooks.com/",
            "freecomputerbooks.com/index.html",
            "freecomputerbooks.com/:category.html",
        ],
        target: "/:category",
    }],
    name: "Free Computer Books - Book List",
    maintainers: &["captura"],
    url: "https://freecomputerbooks.com",
    description: "Book list pages from FreeComputerBooks, aligned with RSSHub /freecomputerbooks/:category? route.",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let category = ctx.param_str("category");
    let limit = ctx.param_i64("limit").unwrap_or(50).max(1) as usize;

    let url = if let Some(cat) = category {
        format!("{}/{}.html", ROOT_URL, cat)
    } else {
        ROOT_URL.to_string()
    };

    let html = util::get_html(&url).await?;
    let (category_title, items) = {
        let doc = Html::parse_document(&html);

        let sel_title = Selector::parse(".maintitlebar")
            .map_err(|e| Error::Parse(format!("freecomputerbooks: title selector error: {e}")))?;
        let sel_item = Selector::parse("ul[id^=\"newBooks\"] > li")
            .map_err(|e| Error::Parse(format!("freecomputerbooks: list selector error: {e}")))?;
        let sel_link = Selector::parse("a")
            .map_err(|e| Error::Parse(format!("freecomputerbooks: link selector error: {e}")))?;

        let category_title = doc
            .select(&sel_title)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_else(|| "Selected New Books".to_string());

        let mut links = Vec::new();
        for li in doc.select(&sel_item).take(limit) {
            let a = match li.select(&sel_link).next() {
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
            let link = util::absolutize(ROOT_URL, href);
            links.push((title, link));
        }
        (category_title, links)
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
        let sel_desc = Selector::parse("#bookdesc")
            .map_err(|e| Error::Parse(format!("freecomputerbooks: desc selector error: {e}")))?;
        let sel_meta = Selector::parse("#booktitle ul")
            .map_err(|e| Error::Parse(format!("freecomputerbooks: meta selector error: {e}")))?;

        let mut description = String::new();
        if let Some(meta) = detail.select(&sel_meta).next() {
            description.push_str(&meta.html());
        }
        if let Some(desc) = detail.select(&sel_desc).next() {
            if !description.is_empty() {
                description.push_str("<br>");
            }
            description.push_str(&desc.html());
        }

        items_out.push(HubItem {
            title,
            description: if description.is_empty() {
                None
            } else {
                Some(description)
            },
            link: Some(link),
            author: None,
            pub_date: None,
            categories: vec![category_title.clone()],
        });
    }

    Ok(HubData {
        title: format!("Free Computer Books - {}", category_title),
        description: Some("FreeComputerBooks selected or category book list.".to_string()),
        link: Some(url),
        image: None,
        language: Some("en-US".to_string()),
        items: items_out,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_FREECOMPUTERBOOKS_INDEX: Route = Route {
    meta: &META_FREECOMPUTERBOOKS_INDEX,
    handler: handler_fn,
};
