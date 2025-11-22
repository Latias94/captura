use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;
use chrono::{DateTime, FixedOffset};
use serde::Deserialize;

const DOCKER_API_BASE: &str = "https://hub.docker.com/v2/repositories";

#[derive(Debug, Deserialize)]
struct DockerImage {
    os: Option<String>,
    architecture: Option<String>,
    digest: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DockerTag {
    name: String,
    tag_last_pushed: String,
    images: Option<Vec<DockerImage>>,
}

#[derive(Debug, Deserialize)]
struct TagsResponse {
    results: Vec<DockerTag>,
}

pub const META_DOCKERHUB_TAG: RouteMeta = RouteMeta {
    hub_id: "dockerhub/tag",
    path: "/dockerhub/tag/:owner/:image/:limits?",
    categories: &["program-update"],
    example: "/dockerhub/tag/library/mariadb",
    params: &[
        ParamMeta {
            name: "owner",
            description: "Image owner (use 'library' for official images)",
            default: None,
            options: &[],
        },
        ParamMeta {
            name: "image",
            description: "Image name",
            default: None,
            options: &[],
        },
        ParamMeta {
            name: "limits",
            description: "Maximum tags to list (default 10)",
            default: Some("10"),
            options: &[],
        },
    ],
    features: Features::basic(),
    radar: &[Radar {
        source: &["hub.docker.com/r"],
        target: "/r/:owner/:image",
    }],
    name: "Docker Image Tags",
    maintainers: &["captura"],
    url: "https://hub.docker.com",
    description: "Docker Hub image tags and architectures, aligned with RSSHub /dockerhub/tag.",
    default_view: Some("program-update"),
};

fn parse_datetime(s: &str) -> Option<DateTime<FixedOffset>> {
    crate::routes::util::parse_date(s)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let owner = ctx.param_str("owner").unwrap_or("").trim();
    let image = ctx.param_str("image").unwrap_or("").trim();
    let limits = ctx.param_i64("limits").unwrap_or(10).max(1) as usize;

    if owner.is_empty() || image.is_empty() {
        return Err(Error::Config(
            "dockerhub/tag: parameters `owner` and `image` are required".to_string(),
        ));
    }

    let namespace = format!("{}/{}", owner, image);
    let repo_link = format!("https://hub.docker.com/r/{}", namespace);

    let client = client_basic(None, None)
        .map_err(|e| Error::Network(format!("dockerhub client error: {}", e)))?;

    let api_url = format!("{}/{}/tags/", DOCKER_API_BASE, namespace);
    let resp = client
        .get(&api_url)
        .query(&[("page_size", limits.to_string())])
        .send()
        .await
        .map_err(|e| Error::Network(format!("{api_url} -> {e}")))?;
    if !resp.status().is_success() {
        return Err(Error::Network(format!(
            "{api_url} -> http status {}",
            resp.status()
        )));
    }
    let data: TagsResponse = resp.json().await.map_err(|e| Error::Parse(e.to_string()))?;

    // Fetch repository description (best effort).
    let mut repo_description = String::new();
    let meta_url = format!("{}/{}/", DOCKER_API_BASE, namespace);
    if let Ok(meta_resp) = client.get(&meta_url).send().await {
        if meta_resp.status().is_success() {
            if let Ok(v) = meta_resp.json::<serde_json::Value>().await {
                if let Some(desc) = v.get("description").and_then(|d| d.as_str()) {
                    repo_description = desc.to_string();
                }
            }
        }
    }

    let mut items = Vec::new();
    for tag in data.results {
        let architectures = tag
            .images
            .as_ref()
            .map(|imgs| {
                imgs.iter()
                    .map(|img| {
                        format!(
                            "{}/{}",
                            img.os.as_deref().unwrap_or("unknown"),
                            img.architecture.as_deref().unwrap_or("unknown")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_else(|| "unknown architectures".to_string());

        let digest = tag
            .images
            .as_ref()
            .and_then(|imgs| imgs.get(0))
            .and_then(|img| img.digest.clone())
            .unwrap_or_default()
            .replace(':', "-");

        let layer_link = if owner == "library" {
            format!(
                "https://hub.docker.com/layers/{image}/{namespace}/{tag}/images/{digest}",
                image = image,
                namespace = namespace,
                tag = tag.name,
                digest = digest
            )
        } else {
            format!(
                "https://hub.docker.com/layers/{namespace}/{tag}/images/{digest}",
                namespace = namespace,
                tag = tag.name,
                digest = digest
            )
        };

        let title = format!(
            "{namespace}:{tag} was updated",
            namespace = namespace,
            tag = tag.name
        );
        let description = format!(
            "{namespace}:{tag} was updated, supporting the {architectures}",
            namespace = namespace,
            tag = tag.name,
            architectures = architectures
        );

        let pub_date = parse_datetime(&tag.tag_last_pushed);

        items.push(HubItem {
            title,
            description: Some(description),
            link: Some(layer_link),
            author: Some(owner.to_string()),
            pub_date,
            categories: vec!["dockerhub".to_string(), "tag".to_string()],
        });
    }

    Ok(HubData {
        title: format!("{namespace} tags", namespace = namespace),
        description: Some(repo_description),
        link: Some(repo_link),
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
pub const ROUTE_DOCKERHUB_TAG: Route = Route {
    meta: &META_DOCKERHUB_TAG,
    handler: handler_fn,
};
