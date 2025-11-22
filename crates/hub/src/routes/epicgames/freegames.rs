use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset, Utc};
use serde::Deserialize;

const ROOT_URL: &str = "https://store.epicgames.com";
const API_BASE: &str = "https://store-site-backend-static-ipv4.ak.epicgames.com";

fn now_utc() -> DateTime<Utc> {
    Utc::now()
}

fn to_fixed_offset(dt: DateTime<Utc>) -> Option<DateTime<FixedOffset>> {
    FixedOffset::east_opt(0).map(|offset| dt.with_timezone(&offset))
}

#[derive(Debug, Deserialize)]
struct CatalogResponse {
    data: CatalogData,
}

#[derive(Debug, Deserialize)]
struct CatalogData {
    #[serde(rename = "Catalog")]
    catalog: Catalog,
}

#[derive(Debug, Deserialize)]
struct Catalog {
    #[serde(rename = "searchStore")]
    search_store: SearchStore,
}

#[derive(Debug, Deserialize)]
struct SearchStore {
    elements: Vec<Element>,
}

#[derive(Debug, Deserialize)]
struct Element {
    title: String,
    description: Option<String>,
    #[serde(default)]
    seller: Seller,
    #[serde(default)]
    categories: Vec<Category>,
    #[serde(default)]
    keyImages: Vec<KeyImage>,
    #[serde(default)]
    catalogNs: CatalogNs,
    #[serde(default)]
    offerMappings: Vec<OfferMapping>,
    #[serde(default)]
    productSlug: Option<String>,
    #[serde(default)]
    urlSlug: Option<String>,
    #[serde(default)]
    offerType: Option<String>,
    #[serde(default)]
    promotions: Option<Promotions>,
}

#[derive(Debug, Default, Deserialize)]
struct Seller {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Default, Deserialize)]
struct Category {
    #[serde(default)]
    path: String,
}

#[derive(Debug, Default, Deserialize)]
struct KeyImage {
    #[serde(default)]
    #[allow(dead_code)]
    r#type: String,
    #[serde(default)]
    url: String,
}

#[derive(Debug, Default, Deserialize)]
struct CatalogNs {
    #[serde(default)]
    mappings: Vec<Mapping>,
}

#[derive(Debug, Default, Deserialize)]
struct Mapping {
    #[serde(default)]
    pageSlug: String,
}

#[derive(Debug, Default, Deserialize)]
struct OfferMapping {
    #[serde(default)]
    pageSlug: String,
}

#[derive(Debug, Deserialize)]
struct Promotions {
    #[serde(default)]
    promotionalOffers: Vec<PromotionWrapper>,
}

#[derive(Debug, Deserialize)]
struct PromotionWrapper {
    #[serde(default)]
    promotionalOffers: Vec<Promotion>,
}

#[derive(Debug, Deserialize)]
struct Promotion {
    startDate: String,
    endDate: String,
    discountSetting: DiscountSetting,
}

#[derive(Debug, Deserialize)]
struct DiscountSetting {
    discountType: String,
    discountPercentage: i64,
}

pub const META_EPICGAMES_FREEGAMES: RouteMeta = RouteMeta {
    hub_id: "epicgames/freegames",
    path: "/epicgames/freegames/:locale?/:country?",
    categories: &["game"],
    example: "/epicgames/freegames/en-US/US",
    params: &[
        ParamMeta {
            name: "locale",
            description: "区域语言代码，默认 en-US，例如 zh-CN。",
            default: Some("en-US"),
            options: &[],
        },
        ParamMeta {
            name: "country",
            description: "国家 / 地区代码，默认 US，例如 CN。",
            default: Some("US"),
            options: &[],
        },
    ],
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
        source: &["store.epicgames.com/:locale/free-games"],
        target: "/freegames/:locale",
    }],
    name: "Epic Games Store - Free Games",
    maintainers: &["captura"],
    url: "https://store.epicgames.com",
    description: "Epic Games Store 当前限免游戏列表，基于官方 freeGamesPromotions 接口。",
    default_view: Some("notifications"),
};

fn build_api_url(locale: &str, country: &str) -> String {
    format!(
        "{base}/freeGamesPromotions?locale={locale}&country={country}&allowCountries={country}",
        base = API_BASE,
        locale = locale,
        country = country
    )
}

fn build_current_url(locale: &str) -> String {
    format!("{}/{}/free-games?lang={}", ROOT_URL, locale, locale)
}

fn is_bundle(element: &Element) -> bool {
    element.categories.iter().any(|c| c.path == "bundles")
}

fn choose_image(element: &Element) -> Option<String> {
    if element.keyImages.is_empty() {
        return None;
    }
    if let Some(wide) = element
        .keyImages
        .iter()
        .find(|k| k.r#type == "DieselStoreFrontWide")
    {
        if !wide.url.is_empty() {
            return Some(wide.url.clone());
        }
    }
    Some(element.keyImages[0].url.clone())
}

fn resolve_slug(element: &Element) -> Option<String> {
    if let Some(m) = element.catalogNs.mappings.get(0) {
        if !m.pageSlug.is_empty() {
            return Some(m.pageSlug.clone());
        }
    }
    if let Some(m) = element.offerMappings.get(0) {
        if !m.pageSlug.is_empty() {
            return Some(m.pageSlug.clone());
        }
    }
    if let Some(slug) = element.productSlug.as_ref() {
        if !slug.is_empty() {
            return Some(slug.clone());
        }
    }
    if let Some(slug) = element.urlSlug.as_ref() {
        if !slug.is_empty() {
            return Some(slug.clone());
        }
    }
    None
}

fn extract_active_promotion(element: &Element) -> Option<&Promotion> {
    let promos = element.promotions.as_ref()?;
    let wrapper = promos.promotionalOffers.get(0)?;
    let promo = wrapper.promotionalOffers.get(0)?;
    if promo.discountSetting.discountType != "PERCENTAGE"
        || promo.discountSetting.discountPercentage != 0
    {
        return None;
    }

    let now = now_utc();
    let start = util::parse_date(&promo.startDate)?;
    let end = util::parse_date(&promo.endDate)?;
    let now_fixed = to_fixed_offset(now)?;

    if start <= now_fixed && end > now_fixed {
        Some(promo)
    } else {
        None
    }
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let locale = ctx.param_str("locale").unwrap_or("en-US");
    let country = ctx.param_str("country").unwrap_or("US");

    let api_url = build_api_url(locale, country);
    let current_url = build_current_url(locale);

    let resp: CatalogResponse = util::get_json(&api_url).await?;

    let mut items = Vec::new();

    for element in resp.data.catalog.search_store.elements.into_iter() {
        let promo = match extract_active_promotion(&element) {
            Some(p) => p,
            None => continue,
        };

        let slug = match resolve_slug(&element) {
            Some(s) => s,
            None => continue,
        };

        let bundle = is_bundle(&element);
        let mut link = if bundle {
            format!("{}/{}/bundles/", ROOT_URL, locale)
        } else {
            format!("{}/{}/p/", ROOT_URL, locale)
        };
        link.push_str(&slug);

        let mut description = String::new();

        if let Some(img) = choose_image(&element) {
            description.push_str("<p>");
            description.push_str(&crate::routes::util::html_img(&img, &element.title));
            description.push_str("</p>");
        }

        if let Some(desc) = element.description.as_ref() {
            if !desc.is_empty() {
                description.push_str("<p>");
                description.push_str(desc);
                description.push_str("</p>");
            }
        }

        if let Some(end_dt) = util::parse_date(&promo.endDate) {
            description.push_str("<p>Free until ");
            description.push_str(&end_dt.to_rfc3339());
            description.push_str(".</p>");
        }

        let pub_date = util::parse_date(&promo.startDate);

        let author = if element.seller.name.is_empty() {
            None
        } else {
            Some(element.seller.name.clone())
        };

        let mut categories = Vec::new();
        categories.push("epicgames".to_string());
        categories.push("free".to_string());

        items.push(HubItem {
            title: element.title.clone(),
            description: if description.is_empty() {
                None
            } else {
                Some(description)
            },
            link: Some(link),
            author,
            pub_date,
            categories,
        });
    }

    if items.is_empty() {
        return Err(Error::NotFound(
            "epicgames/freegames: no active free games found".to_string(),
        ));
    }

    Ok(HubData {
        title: "Epic Games Store - Free Games".to_string(),
        description: Some("Epic Games Store 当前限免游戏列表。".to_string()),
        link: Some(current_url),
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
pub const ROUTE_EPICGAMES_FREEGAMES: Route = Route {
    meta: &META_EPICGAMES_FREEGAMES,
    handler: handler_fn,
};
