use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;
use chrono::{DateTime, FixedOffset};
use serde::Deserialize;
use serde_json::Value;

const CRATES_API_BASE: &str = "https://crates.io/api/v1/crates";

#[derive(Debug, Deserialize)]
struct CrateInfo {
    name: String,
    description: Option<String>,
    repository: Option<String>,
    homepage: Option<String>,
    documentation: Option<String>,
    max_version: String,
    downloads: i64,
    recent_downloads: Option<i64>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Deserialize)]
struct CrateResponse {
    #[serde(rename = "crate")]
    krate: CrateInfo,
}

#[derive(Debug, Deserialize)]
struct CrateVersion {
    num: String,
    created_at: String,
    downloads: i64,
    yanked: bool,
}

#[derive(Debug, Deserialize)]
struct CrateVersionsResponse {
    versions: Vec<CrateVersion>,
}

pub const META_CRATES_CRATE: RouteMeta = RouteMeta {
    hub_id: "crates/crate",
    path: "/crates/:name",
    categories: &["program-update"],
    example: "/crates/tokio",
    params: &[ParamMeta {
        name: "name",
        description: "Crate name on crates.io",
        default: None,
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["crates.io"],
        target: "/crates/:name",
    }],
    name: "Crate Info",
    maintainers: &["captura"],
    url: "https://crates.io",
    description: "Crate metadata and recent versions from crates.io.",
    default_view: Some("program-update"),
};

fn parse_datetime(s: &str) -> Option<DateTime<FixedOffset>> {
    crate::routes::util::parse_date(s)
}

fn render_description(info: &CrateInfo, versions: &[CrateVersion]) -> String {
    let mut html = String::new();

    let crates_link = format!("https://crates.io/crates/{}", info.name);
    html.push_str(&format!(
        "<p><strong>{name}</strong> on <a href=\"{link}\">crates.io</a></p>",
        name = info.name,
        link = crates_link
    ));

    if let Some(desc) = &info.description {
        if !desc.trim().is_empty() {
            html.push_str("<p>");
            html.push_str(&html_escape::encode_safe(desc.trim()));
            html.push_str("</p>");
        }
    }

    html.push_str("<ul>");
    html.push_str(&format!(
        "<li>Latest version: <code>{}</code></li>",
        info.max_version
    ));
    html.push_str(&format!("<li>Total downloads: {}</li>", info.downloads));
    if let Some(rd) = info.recent_downloads {
        html.push_str(&format!("<li>Recent downloads (90 days): {}</li>", rd));
    }
    if let Some(repo) = &info.repository {
        if !repo.is_empty() {
            html.push_str(&format!(
                "<li>Repository: <a href=\"{url}\">{url}</a></li>",
                url = repo
            ));
        }
    }
    if let Some(home) = &info.homepage {
        if !home.is_empty() {
            html.push_str(&format!(
                "<li>Homepage: <a href=\"{url}\">{url}</a></li>",
                url = home
            ));
        }
    }
    if let Some(doc) = &info.documentation {
        if !doc.is_empty() {
            html.push_str(&format!(
                "<li>Docs: <a href=\"{url}\">{url}</a></li>",
                url = doc
            ));
        }
    }
    html.push_str("</ul>");

    if !versions.is_empty() {
        html.push_str("<h3>Recent versions</h3><ul>");
        for v in versions.iter().take(10) {
            let created = v.created_at.as_str();
            let created_short = created.split('T').next().unwrap_or(created);
            html.push_str(&format!(
                "<li><code>{ver}</code> ({date}) - downloads: {dl}{yanked}</li>",
                ver = v.num,
                date = created_short,
                dl = v.downloads,
                yanked = if v.yanked { " [yanked]" } else { "" }
            ));
        }
        html.push_str("</ul>");
    }

    html
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let name = ctx.param_str("name").unwrap_or("").trim();
    if name.is_empty() {
        return Err(Error::Config(
            "crates/crate: parameter `name` is required".to_string(),
        ));
    }

    let client = client_basic(None, None)
        .map_err(|e| Error::Network(format!("crates.io client error: {}", e)))?;

    // Fetch crate metadata.
    let meta_url = format!("{}/{}", CRATES_API_BASE, name);
    let resp = client
        .get(&meta_url)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{meta_url} -> {e}")))?;
    if !resp.status().is_success() {
        return Err(Error::Network(format!(
            "{meta_url} -> http status {}",
            resp.status()
        )));
    }
    let body = resp
        .text()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;
    let crate_resp: CrateResponse =
        serde_json::from_str(&body).map_err(|e| Error::Parse(e.to_string()))?;
    let info = crate_resp.krate;

    // Fetch versions list (best effort).
    let versions_url = format!("{}/{}/versions", CRATES_API_BASE, name);
    let versions: Vec<CrateVersion> = match client.get(&versions_url).send().await {
        Ok(resp) if resp.status().is_success() => {
            let text = resp
                .text()
                .await
                .map_err(|e| Error::Network(e.to_string()))?;
            match serde_json::from_str::<CrateVersionsResponse>(&text) {
                Ok(v) => v.versions,
                Err(_) => Vec::new(),
            }
        }
        _ => Vec::new(),
    };

    let description_html = render_description(&info, &versions);

    let crates_link = format!("https://crates.io/crates/{}", info.name);
    let pub_date = parse_datetime(&info.updated_at).or_else(|| parse_datetime(&info.created_at));

    let item = HubItem {
        title: format!("{} {}", info.name, info.max_version),
        description: Some(description_html),
        link: Some(crates_link.clone()),
        author: None,
        pub_date,
        categories: vec!["crates.io".to_string(), "crate".to_string()],
    };

    Ok(HubData {
        title: format!("{} on crates.io", info.name),
        description: Some(
            info.description
                .clone()
                .unwrap_or_else(|| "Crate metadata and versions from crates.io".to_string()),
        ),
        link: Some(crates_link),
        image: None,
        language: Some("en".to_string()),
        items: vec![item],
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_CRATES_CRATE: Route = Route {
    meta: &META_CRATES_CRATE,
    handler: handler_fn,
};
