use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use serde::Deserialize;
use std::collections::HashMap;
use std::fmt::Write as _;

const NPM_DOWNLOADS_BASE: &str = "https://api.npmjs.org/downloads/point";
const NPM_REGISTRY_BASE: &str = "https://registry.npmjs.org";

#[derive(Debug, Deserialize)]
struct NpmDownloads {
    downloads: i64,
}

#[derive(Debug, Deserialize)]
struct NpmPackageTime {
    time: HashMap<String, String>,
}

pub const META_NPM_PACKAGE: RouteMeta = RouteMeta {
    hub_id: "npm/package",
    // Path is primarily for documentation; actual parameters are passed via query,
    // e.g. captura_hub://npm/package?name=rsshub
    path: "/npm/package/:name",
    categories: &["program-update"],
    example: "/npm/package?name=rsshub",
    params: &[ParamMeta {
        name: "name",
        description: "NPM package name, supports scoped packages like @scope/name",
        default: None,
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["www.npmjs.com/package"],
        target: "/package/:name",
    }],
    name: "NPM Package",
    maintainers: &["captura"],
    url: "https://www.npmjs.com/",
    description: "NPM package snapshot with downloads (day/week/month) and recent version timeline.",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let name = ctx.param_str("name").unwrap_or("").trim();
    if name.is_empty() {
        return Err(Error::Config(
            "npm/package: parameter `name` is required".to_string(),
        ));
    }

    // Fetch download statistics (day/week/month) and package time metadata.
    let url_day = format!("{}/last-day/{}", NPM_DOWNLOADS_BASE, name);
    let url_week = format!("{}/last-week/{}", NPM_DOWNLOADS_BASE, name);
    let url_month = format!("{}/last-month/{}", NPM_DOWNLOADS_BASE, name);
    let url_pkg = format!("{}/{}", NPM_REGISTRY_BASE, name);

    let downloads_day: NpmDownloads = util::get_json(&url_day).await?;
    let downloads_week: NpmDownloads = util::get_json(&url_week).await?;
    let downloads_month: NpmDownloads = util::get_json(&url_month).await?;
    let pkg_time: NpmPackageTime = util::get_json(&url_pkg).await?;

    // Build a version list sorted descending by time, ignoring special keys.
    let mut versions: Vec<(String, String)> = pkg_time
        .time
        .iter()
        .filter_map(|(k, v)| {
            if k == "created" || k == "modified" {
                None
            } else {
                Some((k.clone(), v.clone()))
            }
        })
        .collect();
    versions.sort_by(|a, b| b.1.cmp(&a.1));

    // Parse last modified timestamp for pub_date when available.
    let modified = pkg_time.time.get("modified").cloned();
    let pub_date = modified.as_deref().and_then(|s| util::parse_date(s));

    let description = render_description(
        name,
        downloads_day.downloads,
        downloads_week.downloads,
        downloads_month.downloads,
        &versions,
    );

    let link = format!("https://www.npmjs.com/package/{}", name);

    let item = HubItem {
        title: format!("{name} - npm"),
        description: Some(description),
        link: Some(link.clone()),
        author: None,
        pub_date,
        // Categories include a generic tag so that clients can group NPM feeds.
        categories: vec!["npm".to_string(), "package".to_string()],
    };

    Ok(HubData {
        title: format!("{name} - npm"),
        description: Some(format!("NPM package: {name}")),
        link: Some(link),
        image: None,
        language: Some("en".to_string()),
        items: vec![item],
        // It is possible for very new packages to have zero downloads; still return a feed.
        allow_empty: true,
    })
}

fn render_description(
    name: &str,
    last_day: i64,
    last_week: i64,
    last_month: i64,
    versions: &[(String, String)],
) -> String {
    let mut html = String::new();

    let _ = write!(
        html,
        "<p><strong>{name}</strong> on <a href=\"https://www.npmjs.com/\">npm</a></p>"
    );

    let _ = write!(
        html,
        "<ul>\
         <li>Downloads (last day): {last_day}</li>\
         <li>Downloads (last week): {last_week}</li>\
         <li>Downloads (last month): {last_month}</li>\
         </ul>"
    );

    if !versions.is_empty() {
        let _ = write!(html, "<h3>Versions</h3><ul>");
        for (ver, ts) in versions.iter().take(20) {
            let _ = write!(html, "<li><code>{ver}</code> - {ts}</li>");
        }
        let _ = write!(html, "</ul>");
    }

    html
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_NPM_PACKAGE: Route = Route {
    meta: &META_NPM_PACKAGE,
    handler: handler_fn,
};
