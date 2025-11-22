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
    size: Option<u64>,
    digest: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DockerTag {
    name: String,
    last_updated: String,
    images: Vec<DockerImage>,
}

pub const META_DOCKERHUB_BUILD: RouteMeta = RouteMeta {
    hub_id: "dockerhub/build",
    path: "/dockerhub/build/:owner/:image/:tag?",
    categories: &["program-update"],
    example: "/dockerhub/build/diygod/rsshub/latest",
    params: &[
        ParamMeta {
            name: "owner",
            description: "Image owner (use 'library' for official images, e.g. library/mysql)",
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
            name: "tag",
            description: "Image tag (default latest)",
            default: Some("latest"),
            options: &[],
        },
    ],
    features: Features::basic(),
    radar: &[Radar {
        source: &["hub.docker.com/r"],
        target: "/r/:owner/:image",
    }],
    name: "Docker Image Build",
    maintainers: &["captura"],
    url: "https://hub.docker.com",
    description: "Docker Hub image build history for a specific tag.",
    default_view: Some("program-update"),
};

fn parse_datetime(s: &str) -> Option<DateTime<FixedOffset>> {
    crate::routes::util::parse_date(s)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let owner = ctx.param_str("owner").unwrap_or("").trim();
    let image = ctx.param_str("image").unwrap_or("").trim();
    let tag = ctx.param_str("tag").unwrap_or("latest").trim();

    if owner.is_empty() || image.is_empty() {
        return Err(Error::Config(
            "dockerhub/build: parameters `owner` and `image` are required".to_string(),
        ));
    }

    let namespace = format!("{}/{}", owner, image);
    let tag_name = if tag.is_empty() { "latest" } else { tag };

    let client = client_basic(None, None)
        .map_err(|e| Error::Network(format!("dockerhub client error: {}", e)))?;

    let tag_url = format!("{}/{}/tags/{}", DOCKER_API_BASE, namespace, tag_name);
    let metadata_url = format!("{}/{}/", DOCKER_API_BASE, namespace);

    // Fetch tag metadata.
    let tag_resp = client
        .get(&tag_url)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{tag_url} -> {e}")))?;
    if !tag_resp.status().is_success() {
        return Err(Error::Network(format!(
            "{tag_url} -> http status {}",
            tag_resp.status()
        )));
    }
    let tag_json = tag_resp
        .json::<DockerTag>()
        .await
        .map_err(|e| Error::Parse(e.to_string()))?;

    // Fetch repository description (best effort).
    let mut description_text = String::new();
    if let Ok(meta_resp) = client.get(&metadata_url).send().await {
        if meta_resp.status().is_success() {
            if let Ok(v) = meta_resp.json::<serde_json::Value>().await {
                if let Some(desc) = v.get("description").and_then(|d| d.as_str()) {
                    description_text = desc.to_string();
                }
            }
        }
    }

    let repo_link = format!("https://hub.docker.com/r/{}", namespace);

    let first_image = tag_json.images.get(0);
    let size_mb = first_image
        .and_then(|img| img.size)
        .map(|s| (s as f64) / 1_000_000.0);
    let digest = first_image
        .and_then(|img| img.digest.clone())
        .unwrap_or_default()
        .replace(':', "-");

    let layer_link = if owner == "library" {
        format!(
            "https://hub.docker.com/layers/docker/{namespace}/{tag}/images/{digest}",
            namespace = namespace,
            tag = tag_name,
            digest = digest
        )
    } else {
        format!(
            "https://hub.docker.com/layers/{namespace}/{tag}/images/{digest}",
            namespace = namespace,
            tag = tag_name,
            digest = digest
        )
    };

    let mut item_title = format!(
        "{namespace}:{tag} was built",
        namespace = namespace,
        tag = tag_name
    );
    if let Some(size_mb) = size_mb {
        item_title.push_str(&format!(" ({:.2} MB)", size_mb));
    }

    let pub_date = parse_datetime(&tag_json.last_updated);

    let mut desc_html = String::new();
    if !description_text.is_empty() {
        desc_html.push_str("<p>");
        desc_html.push_str(&html_escape::encode_safe(&description_text));
        desc_html.push_str("</p>");
    }

    let item = HubItem {
        title: item_title,
        description: if desc_html.is_empty() {
            None
        } else {
            Some(desc_html)
        },
        link: Some(layer_link),
        author: Some(owner.to_string()),
        pub_date,
        categories: vec!["dockerhub".to_string(), "build".to_string()],
    };

    Ok(HubData {
        title: format!(
            "{namespace}:{tag} build history",
            namespace = namespace,
            tag = tag_name
        ),
        description: Some(description_text),
        link: Some(repo_link),
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
pub const ROUTE_DOCKERHUB_BUILD: Route = Route {
    meta: &META_DOCKERHUB_BUILD,
    handler: handler_fn,
};
