use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_hub_macros::register_hub_route;
use scraper::{Html, Selector};

const BASE_URL: &str = "https://store.steampowered.com/search/";

pub const META_STEAM_SEARCH: RouteMeta = RouteMeta {
    hub_id: "steam/search",
    path: "/steam/search/:params?",
    categories: &["game"],
    example: "/steam/search/sort_by=Released_DESC&specials=1&os=win&supportedlang=schinese",
    params: &[ParamMeta {
        name: "params",
        description: "Query string for Steam Store search, e.g. sort_by=Released_DESC&specials=1&os=win.",
        default: Some("sort_by=Released_DESC"),
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &[
            "store.steampowered.com",
            "store.steampowered.com/search",
            "store.steampowered.com/search/*",
        ],
        target: "/search/:params?",
    }],
    name: "Steam - Store Search",
    maintainers: &["captura"],
    url: "https://store.steampowered.com",
    description: "Steam Store search results, can be used for new releases and discounts via query parameters.",
    default_view: Some("games"),
};

pub async fn run_search_with_params(params: &str) -> captura_common::Result<HubData> {
    let url = if params.is_empty() {
        BASE_URL.to_string()
    } else {
        format!("{}?{}", BASE_URL, params)
    };

    let html = crate::routes::util::get_html(&url).await?;
    let doc = Html::parse_document(&html);

    let container_sel = Selector::parse("#search_result_container").unwrap();
    let a_sel = Selector::parse("a").unwrap();
    let title_sel = Selector::parse("span.title").unwrap();
    let img_sel = Selector::parse(".search_capsule img").unwrap();
    let discount_pct_sel = Selector::parse(".discount_pct").unwrap();
    let discount_original_sel = Selector::parse(".discount_original_price").unwrap();
    let discount_final_sel = Selector::parse(".discount_final_price").unwrap();
    let review_sel = Selector::parse(".search_review_summary").unwrap();

    let mut items = Vec::new();

    if let Some(container) = doc.select(&container_sel).next() {
        for a in container.select(&a_sel) {
            let title_el = match a.select(&title_sel).next() {
                Some(t) => t,
                None => continue,
            };
            let title = crate::routes::util::element_text(&title_el);
            if title.is_empty() {
                continue;
            }

            let link = a.value().attr("href").map(|href| href.to_string());

            let thumb = a
                .select(&img_sel)
                .next()
                .and_then(|img| img.value().attr("src"))
                .map(|s| s.to_string());

            let is_discounted = a.select(&discount_original_sel).next().is_some();

            let mut desc_parts: Vec<String> = Vec::new();

            if is_discounted {
                if let Some(pct_el) = a.select(&discount_pct_sel).next() {
                    let pct = crate::routes::util::element_text(&pct_el);
                    if !pct.is_empty() {
                        desc_parts.push(format!("Discount: {}", pct));
                    }
                }
                if let Some(orig_el) = a.select(&discount_original_sel).next() {
                    let orig = crate::routes::util::element_text(&orig_el);
                    if !orig.is_empty() {
                        desc_parts.push(format!("Original price: {}", orig));
                    }
                }
                if let Some(final_el) = a.select(&discount_final_sel).next() {
                    let final_price = crate::routes::util::element_text(&final_el);
                    if !final_price.is_empty() {
                        desc_parts.push(format!("Discounted price: {}", final_price));
                    }
                }
            } else if let Some(final_el) = a.select(&discount_final_sel).next() {
                let price = crate::routes::util::element_text(&final_el);
                if !price.is_empty() {
                    desc_parts.push(format!("Price: {}", price));
                }
            }

            if let Some(review_el) = a.select(&review_sel).next() {
                if let Some(tt) = review_el.value().attr("data-tooltip-html") {
                    if !tt.is_empty() {
                        desc_parts.push(tt.to_string());
                    }
                }
            }

            let mut description = String::new();
            if let Some(thumb_url) = thumb {
                description.push_str("<p>");
                description.push_str(&crate::routes::util::html_img(&thumb_url, &title));
                description.push_str("</p>");
            }
            if !desc_parts.is_empty() {
                description.push_str(&desc_parts.join("<br>"));
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
                categories: vec!["steam".to_string(), "store".to_string()],
            });
        }
    }

    Ok(HubData {
        title: "Steam Store search result".to_string(),
        description: Some(format!("Query: {}", params)),
        link: Some(url),
        image: None,
        language: Some("en".to_string()),
        items,
        allow_empty: false,
    })
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let params = ctx.param_str("params").unwrap_or("sort_by=Released_DESC");
    run_search_with_params(params).await
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_STEAM_SEARCH: Route = Route {
    meta: &META_STEAM_SEARCH,
    handler: handler_fn,
};
