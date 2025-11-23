use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset};
use scraper::{Html, Selector};

const BASE_URL: &str = "https://pkgs.alpinelinux.org";

pub const META_ALPINELINUX_PKGS: RouteMeta = RouteMeta {
    hub_id: "alpinelinux/pkgs",
    path: "/alpinelinux/pkgs/:name/:routeParams?",
    categories: &["program-update"],
    example: "/alpinelinux/pkgs?name=nodejs&routeParams=branch=edge&repo=main&arch=x86_64",
    params: &[
        ParamMeta {
            name: "name",
            description: "Package name",
            default: None,
            options: &[],
        },
        ParamMeta {
            name: "routeParams",
            description: "Raw query string for filters, e.g. branch=edge&repo=main&arch=x86_64&maintainer=Jakub%20Jirutka",
            default: Some(""),
            options: &[],
        },
    ],
    features: Features::basic(),
    radar: &[Radar {
        source: &["pkgs.alpinelinux.org/packages"],
        target: "/pkgs/:name",
    }],
    name: "Alpine Linux Packages",
    maintainers: &["captura"],
    url: "https://pkgs.alpinelinux.org/packages",
    description: "Alpine Linux package index filtered by name and optional query parameters.",
    default_view: Some("programs"),
};

#[derive(Debug)]
struct RowData {
    package: String,
    package_url: Option<String>,
    description: Option<String>,
    version: String,
    project: Option<String>,
    license: String,
    branch: String,
    repository: String,
    architecture: String,
    maintainer: String,
    build_date: String,
}

fn parse_build_date(s: &str) -> Option<DateTime<FixedOffset>> {
    util::parse_date(s)
}

fn parse_table(html: &str) -> Result<Vec<RowData>, Error> {
    let doc = Html::parse_document(html);
    let sel_row = Selector::parse("tbody tr")
        .map_err(|e| Error::Parse(format!("alpinelinux: row sel: {e}")))?;
    let sel_package = Selector::parse("td.package a")
        .map_err(|e| Error::Parse(format!("alpinelinux: package sel: {e}")))?;
    let sel_version = Selector::parse("td.version")
        .map_err(|e| Error::Parse(format!("alpinelinux: version sel: {e}")))?;
    let sel_url = Selector::parse("td.url a")
        .map_err(|e| Error::Parse(format!("alpinelinux: url sel: {e}")))?;
    let sel_license = Selector::parse("td.license")
        .map_err(|e| Error::Parse(format!("alpinelinux: license sel: {e}")))?;
    let sel_branch = Selector::parse("td.branch")
        .map_err(|e| Error::Parse(format!("alpinelinux: branch sel: {e}")))?;
    let sel_repo = Selector::parse("td.repo a")
        .map_err(|e| Error::Parse(format!("alpinelinux: repo sel: {e}")))?;
    let sel_arch = Selector::parse("td.arch a")
        .map_err(|e| Error::Parse(format!("alpinelinux: arch sel: {e}")))?;
    let sel_maintainer = Selector::parse("td.maintainer a")
        .map_err(|e| Error::Parse(format!("alpinelinux: maintainer sel: {e}")))?;
    let sel_bdate = Selector::parse("td.bdate")
        .map_err(|e| Error::Parse(format!("alpinelinux: bdate sel: {e}")))?;

    let mut rows = Vec::new();
    for row in doc.select(&sel_row) {
        let pkg_a = row.select(&sel_package).next();
        let package = pkg_a
            .as_ref()
            .map(|a| a.text().collect::<String>().trim().to_string())
            .unwrap_or_default();
        if package.is_empty() {
            continue;
        }
        let package_href = pkg_a
            .as_ref()
            .and_then(|a| a.value().attr("href"))
            .map(|s| s.to_string());
        let description = pkg_a
            .as_ref()
            .and_then(|a| a.value().attr("aria-label"))
            .map(|s| s.to_string());

        let version = row
            .select(&sel_version)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();

        let project = row
            .select(&sel_url)
            .next()
            .and_then(|a| a.value().attr("href"))
            .map(|s| s.to_string());

        let license = row
            .select(&sel_license)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();

        let branch = row
            .select(&sel_branch)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();

        let repository = row
            .select(&sel_repo)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();

        let architecture = row
            .select(&sel_arch)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();

        let maintainer = row
            .select(&sel_maintainer)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();

        let build_date = row
            .select(&sel_bdate)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();

        rows.push(RowData {
            package,
            package_url: package_href,
            description,
            version,
            project,
            license,
            branch,
            repository,
            architecture,
            maintainer,
            build_date,
        });
    }

    Ok(rows)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let name = ctx.param_str("name").unwrap_or("").trim();
    if name.is_empty() {
        return Err(Error::Config(
            "alpinelinux/pkgs: parameter `name` is required".to_string(),
        ));
    }
    let route_params_raw = ctx.param_str("routeParams").unwrap_or("").trim();

    // Build query string, preserving existing encoding in routeParams and appending name.
    let mut qs = String::new();
    if !route_params_raw.is_empty() {
        qs.push_str(route_params_raw);
    }
    if !qs.is_empty() && !qs.ends_with('&') {
        qs.push('&');
    }
    qs.push_str("name=");
    qs.push_str(&urlencoding::encode(name));

    let link = format!("{BASE_URL}/packages?{qs}");
    let html = util::get_html(&link).await?;
    let rows = parse_table(&html)?;

    let mut items = Vec::new();
    for row in rows {
        let pkg_link = row.package_url.as_ref().map(|p| format!("{BASE_URL}{p}"));
        let title = format!("{}@{}/{}", row.package, row.version, row.architecture);

        let desc = format!(
            "Version: {}<br>\
             Project: {}<br>\
             Description: {}<br>\
             License: {}<br>\
             Branch: {}<br>\
             Repository: {}<br>\
             Maintainer: {}<br>\
             Build Date: {}",
            row.version,
            row.project.clone().unwrap_or_default(),
            row.description.clone().unwrap_or_default(),
            row.license,
            row.branch,
            row.repository,
            row.maintainer,
            row.build_date,
        );

        let pub_date = parse_build_date(&row.build_date);

        let mut categories = Vec::new();
        categories.push("alpinelinux".to_string());
        if !row.branch.is_empty() {
            categories.push(row.branch.clone());
        }
        if !row.repository.is_empty() {
            categories.push(row.repository.clone());
        }
        if !row.architecture.is_empty() {
            categories.push(row.architecture.clone());
        }

        items.push(HubItem {
            title,
            description: Some(desc),
            link: pkg_link,
            author: Some(row.maintainer),
            pub_date,
            categories,
        });
    }

    Ok(HubData {
        title: format!("{} - Alpine Linux packages", name),
        description: Some("Alpine Linux packages update".to_string()),
        link: Some(link),
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
pub const ROUTE_ALPINELINUX_PKGS: Route = Route {
    meta: &META_ALPINELINUX_PKGS,
    handler: handler_fn,
};
