use crate::routes::types::{
    FeatureConfig, Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::{Error, Result};
use captura_fetcher::{FetchOptions, HttpFetcher};
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset};
use serde::Deserialize;

const GOOGLE_FONTS_API: &str = "https://www.googleapis.com/webfonts/v1/webfonts";

pub const META_GOOGLE_FONTS: RouteMeta = RouteMeta {
    hub_id: "google/fonts",
    path: "/google/fonts/:sort?",
    categories: &["design"],
    example: "/google/fonts/date",
    params: &[ParamMeta {
        name: "sort",
        description:
            "Sorting type, one of date / trending / popularity / alpha / style, default date。",
        default: Some("date"),
        options: &[
            ("date", "Newest"),
            ("trending", "Trending"),
            ("popularity", "Most popular"),
            ("alpha", "Name"),
            ("style", "Number of styles"),
        ],
    }],
    features: Features::with_config(&[FeatureConfig {
        name: "GOOGLE_FONTS_API_KEY",
        description: "Google Fonts API Key, used to access webfonts API。",
        optional: false,
    }]),
    radar: &[Radar {
        source: &["fonts.google.com"],
        target: "/fonts/:sort?",
    }],
    name: "Google Fonts",
    maintainers: &["captura"],
    url: "https://fonts.google.com",
    description: "Google Fonts 字体列表，对标 RSSHub /google/fonts/:sort 路由（需 API Key）。",
    default_view: Some("articles"),
};

#[derive(Debug, Deserialize)]
struct GoogleFontsResp {
    #[serde(default)]
    items: Vec<GoogleFontItem>,
}

#[derive(Debug, Deserialize)]
struct GoogleFontItem {
    family: String,
    #[serde(default)]
    lastModified: String,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    variants: Vec<String>,
}

fn parse_date(raw: &str) -> Option<DateTime<FixedOffset>> {
    crate::routes::util::parse_date(raw)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let sort = ctx.param_str("sort").unwrap_or("date");
    let api_key = std::env::var("GOOGLE_FONTS_API_KEY")
        .map_err(|_| Error::Config("GOOGLE_FONTS_API_KEY is required".to_string()))?;
    if api_key.trim().is_empty() {
        return Err(Error::Config(
            "GOOGLE_FONTS_API_KEY is required".to_string(),
        ));
    }

    let limit = ctx.param_i64("limit").unwrap_or(25).max(1) as usize;

    let url = format!("{}?sort={}&key={}", GOOGLE_FONTS_API, sort, api_key);
    let fetcher = HttpFetcher::new(FetchOptions::default())?;
    let (bytes, _hdrs) = fetcher.fetch_bytes(&url).await?;
    let resp: GoogleFontsResp =
        serde_json::from_slice(&bytes).map_err(|e| Error::Parse(e.to_string()))?;

    let mut items = Vec::new();
    for font in resp.items.into_iter().take(limit) {
        let title = font.family.clone();
        let link = format!(
            "https://fonts.google.com/specimen/{}",
            font.family.replace(' ', "+")
        );
        let pub_date = parse_date(&font.lastModified);

        let mut categories = Vec::new();
        if let Some(cat) = font.category.as_ref() {
            if !cat.is_empty() {
                categories.push(cat.clone());
            }
        }
        if !font.variants.is_empty() {
            categories.push(format!("styles: {}", font.variants.join(",")));
        }

        let description = Some(format!(
            "Family: {}<br>Last modified: {}<br>Variants: {}",
            font.family,
            font.lastModified,
            if font.variants.is_empty() {
                "none".to_string()
            } else {
                font.variants.join(", ")
            }
        ));

        items.push(HubItem {
            title,
            description,
            link: Some(link),
            author: None,
            pub_date,
            categories,
        });
    }

    Ok(HubData {
        title: format!("Google Fonts - {}", sort),
        description: Some("Google Fonts API 列表。".to_string()),
        link: Some("https://fonts.google.com".to_string()),
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
pub const ROUTE_GOOGLE_FONTS: Route = Route {
    meta: &META_GOOGLE_FONTS,
    handler: handler_fn,
};
