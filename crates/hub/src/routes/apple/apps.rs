use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;
use chrono::{DateTime, FixedOffset};
use regex::Regex;
use scraper::{Html, Selector};
use serde_json::Value;

const ROOT_URL: &str = "https://apps.apple.com";

fn parse_date(s: &str) -> Option<DateTime<FixedOffset>> {
    crate::routes::util::parse_date(s)
}

async fn fetch_text(url: &str) -> captura_common::Result<String> {
    let client = client_basic(None, None).map_err(|e| Error::Network(e.to_string()))?;
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{} -> {}", url, e)))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!("{} -> http status {}", url, status)));
    }
    resp.text().await.map_err(|e| Error::Network(e.to_string()))
}

async fn fetch_json(url: &str, bearer: &str) -> captura_common::Result<Value> {
    let client = client_basic(None, None).map_err(|e| Error::Network(e.to_string()))?;
    let resp = client
        .get(url)
        .header("authorization", format!("Bearer {}", bearer))
        .header("origin", ROOT_URL)
        .query(&[
            ("platform", "iphone"),
            (
                "additionalPlatforms",
                "appletv,ipad,iphone,mac,realityDevice,watch",
            ),
            (
                "extend",
                "accessibility,accessibilityDetails,ageRating,backgroundAssetsInfo,backgroundAssetsInfoWithOptional,customArtwork,customDeepLink,customIconArtwork,customPromotionalText,customScreenshotsByType,customVideoPreviewsByType,description,expectedReleaseDateDisplayFormat,fileSizeByDevice,gameDisplayName,iconArtwork,installSizeByDeviceInBytes,messagesScreenshots,miniGamesDeepLink,minimumOSVersion,privacy,privacyDetails,privacyPolicyUrl,remoteControllerRequirement,requirementsByDeviceFamily,supportURLForLanguage,supportedGameCenterFeatures,supportsFunCamera,supportsSharePlay,versionHistory,websiteUrl",
            ),
            ("extend[app-events]", "description,productArtwork,productVideo"),
            (
                "include",
                "alternate-apps,app-bundles,customers-also-bought-apps,developer,developer-other-apps,merchandised-in-apps,related-editorial-items,reviews,top-in-apps",
            ),
            ("include[apps]", "app-events"),
            ("availableIn[app-events]", "future"),
            ("sparseLimit[apps:customers-also-bought-apps]", "40"),
            ("sparseLimit[apps:developer-other-apps]", "40"),
            ("sparseLimit[apps:related-editorial-items]", "40"),
            ("limit[reviews]", "8"),
            ("l", "en-US"),
        ])
        .send()
        .await
        .map_err(|e| Error::Network(format!("{} -> {}", url, e)))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!("{} -> http status {}", url, status)));
    }
    resp.json::<Value>()
        .await
        .map_err(|e| Error::Parse(e.to_string()))
}

async fn fetch_appstore_bearer() -> captura_common::Result<String> {
    // Follow RSSHub's approach: load /us/iphone/today, then fetch the first
    // module script and extract a JWT token from it.
    let today_html = fetch_text(&format!("{ROOT_URL}/us/iphone/today")).await?;
    // Parse the HTML to find the first <script type="module" src="...">.
    let src = {
        let doc = Html::parse_document(&today_html);
        let sel_script = Selector::parse(r#"script[type="module"][src]"#)
            .map_err(|e| Error::Parse(format!("apple/apps: script selector error: {e}")))?;
        let script = doc
            .select(&sel_script)
            .next()
            .ok_or_else(|| Error::Parse("apple/apps: module script not found".to_string()))?;
        script
            .value()
            .attr("src")
            .ok_or_else(|| Error::Parse("apple/apps: script src missing".to_string()))?
            .to_string()
    };
    let module_url = format!("{ROOT_URL}{}", src);
    let js = fetch_text(&module_url).await?;

    let re = Regex::new(r#"="(eyJhbGci[^"]+)""#).map_err(|e| Error::Parse(e.to_string()))?;
    let caps = re
        .captures(&js)
        .ok_or_else(|| Error::Parse("apple/apps: bearer token not found".to_string()))?;
    let token = caps
        .get(1)
        .ok_or_else(|| Error::Parse("apple/apps: bearer token capture missing".to_string()))?
        .as_str()
        .to_string();
    Ok(token)
}

pub const META_APPLE_APPS_UPDATE: RouteMeta = RouteMeta {
    hub_id: "apple/apps/update",
    path: "/apple/apps/update/:country/:id/:platform?",
    categories: &["program-update"],
    example: "/apple/apps/update/us/id408709785",
    params: &[
        ParamMeta {
            name: "country",
            description:
                "App Store country code from the app URL, e.g. us / cn / jp.",
            default: Some("us"),
            options: &[],
        },
        ParamMeta {
            name: "id",
            description:
                "App id from the App Store URL, e.g. id408709785.",
            default: None,
            options: &[],
        },
        ParamMeta {
            name: "platform",
            description:
                "App platform: all (default), iOS, macOS, or tvOS (case-insensitive).",
            default: Some("all"),
            options: &[
                ("all", "All platforms"),
                ("iOS", "iOS"),
                ("macOS", "macOS"),
                ("tvOS", "tvOS"),
            ],
        },
    ],
    features: Features::basic(),
    radar: &[Radar {
        source: &[
            "apps.apple.com/:country/app/:appSlug/:id",
            "apps.apple.com/:country/app/:id",
        ],
        target: "/apps/update/:country/:id",
    }],
    name: "Apple App Store updates",
    maintainers: &["captura"],
    url: "https://apps.apple.com",
    description:
        "App Store version history for a specific app, aligned with RSSHub /apple/apps/update route.",
    default_view: Some("notifications"),
};

fn normalize_platform_id(raw: &str) -> String {
    let lower = raw.to_lowercase();
    match lower.as_str() {
        "ios" => "ios".to_string(),
        "macos" => "osx".to_string(),
        "tvos" => "appletvos".to_string(),
        other => other.to_string(),
    }
}

fn platform_label(internal_id: &str) -> String {
    match internal_id {
        "osx" => "macOS".to_string(),
        "ios" => "iOS".to_string(),
        "appletvos" => "tvOS".to_string(),
        other => other.to_string(),
    }
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let country = ctx.param_str("country").unwrap_or("us");
    let id_raw = ctx
        .param_str("id")
        .ok_or_else(|| Error::Config("apple/apps: missing id parameter".to_string()))?;
    let platform_param = ctx.param_str("platform");
    let limit = ctx.param_i64("limit").unwrap_or(100).max(1) as usize;

    let platform_id_opt = platform_param
        .map(|p| p.trim())
        .filter(|p| !p.eq_ignore_ascii_case("all"))
        .map(normalize_platform_id);

    let current_url = format!("{}/{}/app/{}", ROOT_URL, country, id_raw);
    let bearer = fetch_appstore_bearer().await?;

    let app_id = id_raw.trim_start_matches("id");
    let api_url = format!(
        "https://amp-api-edge.apps.apple.com/v1/catalog/{}/apps/{}",
        country, app_id
    );
    let json = fetch_json(&api_url, &bearer).await?;

    let data = json
        .get("data")
        .and_then(|v| v.as_array())
        .ok_or_else(|| Error::Parse("apple/apps: missing data array".to_string()))?;
    let first = data
        .get(0)
        .ok_or_else(|| Error::Parse("apple/apps: empty data array".to_string()))?;
    let attrs = first
        .get("attributes")
        .and_then(|v| v.as_object())
        .ok_or_else(|| Error::Parse("apple/apps: missing attributes".to_string()))?;

    let app_name = attrs
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or(id_raw)
        .to_string();
    let artist_name = attrs
        .get("artistName")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let platform_attrs = attrs
        .get("platformAttributes")
        .and_then(|v| v.as_object())
        .ok_or_else(|| Error::Parse("apple/apps: missing platformAttributes".to_string()))?;

    let mut items_raw: Vec<(String, Value)> = Vec::new();
    let mut description = String::new();
    let mut image = String::new();
    let mut title_prefix = app_name.clone();

    if let Some(ref pid) = platform_id_opt {
        if let Some(attr) = platform_attrs.get(pid) {
            let label = platform_label(pid);
            title_prefix = format!("{} for {}", app_name, label);

            if let Some(desc_obj) = attr.get("description") {
                if let Some(desc_str) = desc_obj.get("standard").and_then(|v| v.as_str()) {
                    description = desc_str.replace('\n', " ");
                }
            }
            if let Some(icon) = attr.get("iconArtwork") {
                if let Some(url) = icon.get("url").and_then(|v| v.as_str()) {
                    image = url.replace("{w}x{h}{c}.{f}", "3000x3000bb.webp");
                }
            }

            if let Some(history) = attr.get("versionHistory").and_then(|v| v.as_array()) {
                for entry in history {
                    items_raw.push((pid.clone(), entry.clone()));
                }
            }
        }
    } else {
        for (pid, attr) in platform_attrs.iter() {
            if let Some(desc_obj) = attr.get("description") {
                if description.is_empty() {
                    if let Some(desc_str) = desc_obj.get("standard").and_then(|v| v.as_str()) {
                        description = desc_str.replace('\n', " ");
                    }
                }
            }
            if let Some(icon) = attr.get("iconArtwork") {
                if image.is_empty() {
                    if let Some(url) = icon.get("url").and_then(|v| v.as_str()) {
                        image = url.replace("{w}x{h}{c}.{f}", "3000x3000bb.webp");
                    }
                }
            }
            if let Some(history) = attr.get("versionHistory").and_then(|v| v.as_array()) {
                for entry in history {
                    items_raw.push((pid.clone(), entry.clone()));
                }
            }
        }
    }

    let mut items = Vec::new();
    for (pid, entry) in items_raw.into_iter().take(limit) {
        let label = platform_label(&pid);
        let version = entry
            .get("versionDisplay")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let ts = entry
            .get("releaseTimestamp")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let pub_date = parse_date(ts);
        let notes = entry
            .get("releaseNotes")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let desc_html = if notes.is_empty() {
            None
        } else {
            Some(notes.replace('\n', "<br>"))
        };

        let title = if version.is_empty() {
            format!("{} for {}", app_name, label)
        } else {
            format!("{} {} for {}", app_name, version, label)
        };

        items.push(HubItem {
            title,
            description: desc_html,
            link: Some(current_url.clone()),
            author: Some(artist_name.clone()),
            pub_date,
            categories: vec![label],
        });
    }

    Ok(HubData {
        title: format!("{} - Apple App Store", title_prefix),
        description: if description.is_empty() {
            None
        } else {
            Some(description)
        },
        link: Some(current_url),
        image: if image.is_empty() {
            None
        } else {
            Some(image.clone())
        },
        language: Some("en-US".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_APPLE_APPS_UPDATE: Route = Route {
    meta: &META_APPLE_APPS_UPDATE,
    handler: handler_fn,
};
