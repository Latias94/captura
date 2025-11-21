use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;
use chrono::{DateTime, FixedOffset};
use scraper::{Html, Selector};

pub const META_JAVBUS_LIST: RouteMeta = RouteMeta {
    hub_id: "javbus",
    path: "/javbus/:category?",
    categories: &["multimedia"],
    example: "/javbus/censored",
    params: &[ParamMeta {
        name: "category",
        description: "Category: censored (default), uncensored, or western.",
        default: Some("censored"),
        options: &[
            ("censored", "Censored (javbus.com)"),
            ("uncensored", "Uncensored (javbus.com/uncensored)"),
            ("western", "Western (javbus.org)"),
        ],
    }],
    features: Features {
        require_config: &[],
        require_puppeteer: false,
        anti_crawler: false,
        support_bt: false,
        support_podcast: false,
        support_scihub: false,
        nsfw: true,
    },
    radar: &[Radar {
        source: &[
            "www.javbus.com/",
            "www.javbus.com/uncensored",
            "www.javbus.org/",
        ],
        target: "/:category?",
    }],
    name: "JavBus list",
    maintainers: &["captura"],
    url: "https://www.javbus.com",
    description: "JavBus list pages for censored / uncensored / western videos (metadata only).",
    default_view: Some("videos"),
};

fn build_base_url(category: &str) -> (&'static str, &'static str) {
    match category {
        "uncensored" => ("https://www.javbus.com/uncensored", "Uncensored"),
        "western" => ("https://www.javbus.org", "Western"),
        _ => ("https://www.javbus.com", "Censored"),
    }
}

fn parse_date(s: &str) -> Option<DateTime<FixedOffset>> {
    // Dates are in YYYY-MM-DD format on JavBus.
    crate::routes::util::parse_date(s)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let category = ctx.param_str("category").unwrap_or("censored");
    let (base_url, label) = build_base_url(category);
    let limit = ctx.param_i64("limit").unwrap_or(30).max(1) as usize;

    let client = client_basic(None, None)
        .map_err(|e| Error::Network(format!("javbus client error: {}", e)))?;
    let resp = client
        .get(base_url)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{base_url} -> {e}")))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "{base_url} -> http status {status}"
        )));
    }
    let html = resp
        .text()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;

    let doc = Html::parse_document(&html);
    let sel_item = Selector::parse("a.movie-box")
        .map_err(|e| Error::Parse(format!("javbus: invalid list selector: {e}")))?;
    let sel_img = Selector::parse("div.photo-frame img")
        .map_err(|e| Error::Parse(format!("javbus: invalid img selector: {e}")))?;
    let sel_date = Selector::parse("date")
        .map_err(|e| Error::Parse(format!("javbus: invalid date selector: {e}")))?;

    let mut items = Vec::new();

    for a in doc.select(&sel_item).take(limit) {
        let href = a
            .value()
            .attr("href")
            .map(|s| s.trim())
            .filter(|s| !s.is_empty());
        let Some(link) = href else {
            continue;
        };

        // First <date> is usually the code, last is the release date.
        let mut date_nodes = a.select(&sel_date);
        let code = date_nodes
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();
        let release_str = date_nodes
            .last()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();
        let pub_date = if release_str.is_empty() {
            None
        } else {
            parse_date(&release_str)
        };

        // Use code as title; do not forward the (often explicit) full title.
        let title = if code.is_empty() {
            link.to_string()
        } else {
            code.clone()
        };

        // Optional cover thumbnail (metadata only, no explicit text).
        let img_url = a
            .select(&sel_img)
            .next()
            .and_then(|img| img.value().attr("src"))
            .map(|src| crate::routes::util::absolutize(base_url, src));

        let mut description = String::new();
        if !release_str.is_empty() {
            description.push_str(&format!("<p>Release date: {}</p>", release_str));
        }
        if let Some(img) = img_url {
            description.push_str("<p>");
            description.push_str(&crate::routes::util::html_img(&img, &title));
            description.push_str("</p>");
        }

        items.push(HubItem {
            title,
            description: if description.is_empty() {
                None
            } else {
                Some(description)
            },
            link: Some(link.to_string()),
            author: None,
            pub_date,
            categories: vec![label.to_string()],
        });
    }

    Ok(HubData {
        title: format!("JavBus - {}", label),
        description: Some(format!("JavBus {} list (metadata only).", label)),
        link: Some(base_url.to_string()),
        image: None,
        language: Some("ja".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_JAVBUS_LIST: Route = Route {
    meta: &META_JAVBUS_LIST,
    handler: handler_fn,
};
