use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;
use chrono::{DateTime, FixedOffset};
use serde::Deserialize;
use serde_json::Value;

const PYPI_API_BASE: &str = "https://pypi.org/pypi";

#[derive(Debug, Deserialize)]
struct PypiInfo {
    name: String,
    summary: Option<String>,
    version: String,
    #[serde(default)]
    home_page: Option<String>,
    #[serde(default)]
    project_url: Option<String>,
    #[serde(default)]
    project_urls: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct PypiReleaseFile {
    filename: String,
    url: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    upload_time_iso_8601: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PypiResponse {
    info: PypiInfo,
    #[serde(default)]
    releases: Value,
}

pub const META_PYPI_PROJECT: RouteMeta = RouteMeta {
    hub_id: "pypi/project",
    path: "/pypi/project/:name",
    categories: &["program-update"],
    example: "/pypi/project/requests",
    params: &[ParamMeta {
        name: "name",
        description: "PyPI project name",
        default: None,
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["pypi.org/project"],
        target: "/project/:name",
    }],
    name: "PyPI Project",
    maintainers: &["captura"],
    url: "https://pypi.org",
    description: "PyPI project metadata and recent releases.",
    default_view: Some("program-update"),
};

fn parse_datetime(s: &str) -> Option<DateTime<FixedOffset>> {
    crate::routes::util::parse_date(s)
}

fn render_description(info: &PypiInfo, latest_version: &str, author_link: Option<&str>) -> String {
    let project_link = format!("https://pypi.org/project/{}/", info.name);
    let mut html = String::new();

    html.push_str(&format!(
        "<p><strong>{name}</strong> on <a href=\"{link}\">PyPI</a></p>",
        name = info.name,
        link = project_link
    ));

    if let Some(summary) = &info.summary {
        if !summary.trim().is_empty() {
            html.push_str("<p>");
            html.push_str(&html_escape::encode_safe(summary.trim()));
            html.push_str("</p>");
        }
    }

    html.push_str("<ul>");
    html.push_str(&format!(
        "<li>Latest version: <code>{}</code></li>",
        latest_version
    ));
    if let Some(home) = &info.home_page {
        if !home.is_empty() {
            html.push_str(&format!(
                "<li>Homepage: <a href=\"{url}\">{url}</a></li>",
                url = home
            ));
        }
    }
    if let Some(project_url) = &info.project_url {
        if !project_url.is_empty() {
            html.push_str(&format!(
                "<li>Project URL: <a href=\"{url}\">{url}</a></li>",
                url = project_url
            ));
        }
    }
    if let Some(project_urls) = &info.project_urls {
        if let Some(map) = project_urls.as_object() {
            if !map.is_empty() {
                html.push_str("<li>Links:<ul>");
                for (k, v) in map {
                    if let Some(url) = v.as_str() {
                        html.push_str(&format!(
                            "<li>{k}: <a href=\"{url}\">{url}</a></li>",
                            k = html_escape::encode_safe(k),
                            url = url
                        ));
                    }
                }
                html.push_str("</ul></li>");
            }
        }
    }
    if let Some(author) = author_link {
        if !author.is_empty() {
            html.push_str(&format!("<li>Author: {}</li>", author));
        }
    }
    html.push_str("</ul>");

    html
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let name = ctx.param_str("name").unwrap_or("").trim();
    if name.is_empty() {
        return Err(Error::Config(
            "pypi/project: parameter `name` is required".to_string(),
        ));
    }

    let client = client_basic(None, None)
        .map_err(|e| Error::Network(format!("pypi client error: {}", e)))?;

    let url = format!("{}/{}/json", PYPI_API_BASE, name);
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{url} -> {e}")))?;
    if !resp.status().is_success() {
        return Err(Error::Network(format!(
            "{url} -> http status {}",
            resp.status()
        )));
    }
    let body = resp
        .text()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;
    let data: PypiResponse =
        serde_json::from_str(&body).map_err(|e| Error::Parse(e.to_string()))?;

    let info = data.info;

    // Determine latest version and release files.
    let latest_version = info.version.clone();
    let releases_value = data.releases;
    let mut latest_files: Vec<PypiReleaseFile> = Vec::new();
    let mut latest_pub_date: Option<DateTime<FixedOffset>> = None;

    if let Some(map) = releases_value.as_object() {
        if let Some(files_val) = map.get(&latest_version) {
            if let Some(files_arr) = files_val.as_array() {
                for file in files_arr {
                    if let Ok(f) = serde_json::from_value::<PypiReleaseFile>(file.clone()) {
                        if let Some(ref ts) = f.upload_time_iso_8601 {
                            if let Some(dt) = parse_datetime(ts) {
                                latest_pub_date = match latest_pub_date {
                                    None => Some(dt),
                                    Some(cur) => Some(cur.max(dt)),
                                };
                            }
                        }
                        latest_files.push(f);
                    }
                }
            }
        }
    }

    let description_html = render_description(&info, &latest_version, None);

    // Build a single item representing the latest release; link to PyPI project.
    let project_link = format!("https://pypi.org/project/{}/", info.name);
    let mut categories = vec!["pypi".to_string(), "python".to_string()];
    categories.push(info.name.clone());

    let mut extra_html = String::new();
    if !latest_files.is_empty() {
        extra_html.push_str("<h3>Files for latest release</h3><ul>");
        for f in latest_files.iter().take(20) {
            let size_mb = f.size as f64 / 1_000_000.0;
            extra_html.push_str(&format!(
                "<li><a href=\"{url}\">{file}</a> ({size:.2} MB)</li>",
                url = f.url,
                file = html_escape::encode_safe(&f.filename),
                size = size_mb,
            ));
        }
        extra_html.push_str("</ul>");
    }

    let full_description = if extra_html.is_empty() {
        description_html
    } else {
        format!("{desc}{extra}", desc = description_html, extra = extra_html)
    };

    let item = HubItem {
        title: format!("{} {}", info.name, latest_version),
        description: Some(full_description),
        link: Some(project_link.clone()),
        author: None,
        pub_date: latest_pub_date,
        categories,
    };

    Ok(HubData {
        title: format!("{} on PyPI", info.name),
        description: Some(
            info.summary
                .clone()
                .unwrap_or_else(|| "PyPI project metadata and latest release.".to_string()),
        ),
        link: Some(project_link),
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
pub const ROUTE_PYPI_PROJECT: Route = Route {
    meta: &META_PYPI_PROJECT,
    handler: handler_fn,
};
