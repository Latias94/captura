use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;
use chrono::FixedOffset;
use serde::Deserialize;

const DOCKER_API_BASE: &str = "https://hub.docker.com/v2/repositories";

#[derive(Debug, Deserialize)]
struct DockerRepository {
    name: String,
    description: Option<String>,
    status_description: Option<String>,
    star_count: Option<i64>,
    pull_count: Option<i64>,
    last_updated: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RepositoriesResponse {
    results: Vec<DockerRepository>,
}

pub const META_DOCKERHUB_REPOSITORIES: RouteMeta = RouteMeta {
    hub_id: "dockerhub/repositories",
    path: "/dockerhub/repositories/:owner",
    categories: &["program-update"],
    example: "/dockerhub/repositories/diygod",
    params: &[
        ParamMeta {
            name: "owner",
            description: "Image owner (namespace on Docker Hub)",
            default: None,
            options: &[],
        },
        ParamMeta {
            name: "limit",
            description: "Maximum repositories to list (default 10)",
            default: Some("10"),
            options: &[],
        },
    ],
    features: Features::basic(),
    radar: &[Radar {
        source: &["hub.docker.com/r"],
        target: "/r/:owner",
    }],
    name: "Docker Hub Owner Repositories",
    maintainers: &["captura"],
    url: "https://hub.docker.com",
    description: "List of repositories for a Docker Hub owner.",
    default_view: Some("program-update"),
};

fn parse_datetime(s: &str) -> Option<chrono::DateTime<FixedOffset>> {
    crate::routes::util::parse_date(s)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let owner = ctx.param_str("owner").unwrap_or("").trim().to_lowercase();
    if owner.is_empty() {
        return Err(Error::Config(
            "dockerhub/repositories: parameter `owner` is required".to_string(),
        ));
    }
    let limit = ctx.param_i64("limit").unwrap_or(10).max(1) as usize;

    let client = client_basic(None, None)
        .map_err(|e| Error::Network(format!("dockerhub client error: {}", e)))?;

    let api_url = format!("{}/{}", DOCKER_API_BASE, owner);
    let resp = client
        .get(&api_url)
        .query(&[("page_size", limit.to_string())])
        .send()
        .await
        .map_err(|e| Error::Network(format!("{api_url} -> {e}")))?;
    if !resp.status().is_success() {
        return Err(Error::Network(format!(
            "{api_url} -> http status {}",
            resp.status()
        )));
    }
    let data: RepositoriesResponse = resp.json().await.map_err(|e| Error::Parse(e.to_string()))?;

    let mut items = Vec::new();
    for repo in data.results {
        let repo_name = repo.name;
        let title = repo_name.clone();
        let desc = format!(
            "{}<br>status: {}<br>stars: {}<br>pulls: {}",
            repo.description.unwrap_or_default(),
            repo.status_description.unwrap_or_default(),
            repo.star_count.unwrap_or(0),
            repo.pull_count.unwrap_or(0),
        );

        let link = format!("https://hub.docker.com/r/{}/{}", owner, repo_name);
        let pub_date = repo.last_updated.as_deref().and_then(|s| parse_datetime(s));

        items.push(HubItem {
            title,
            description: Some(desc),
            link: Some(link),
            author: Some(owner.clone()),
            pub_date,
            categories: vec!["dockerhub".to_string(), "repository".to_string()],
        });
    }

    let owner_link = format!("https://hub.docker.com/r/{}", owner);

    Ok(HubData {
        title: format!("{} repositories", owner),
        description: Some(format!("List of repositories for {}", owner)),
        link: Some(owner_link),
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
pub const ROUTE_DOCKERHUB_REPOSITORIES: Route = Route {
    meta: &META_DOCKERHUB_REPOSITORIES,
    handler: handler_fn,
};
