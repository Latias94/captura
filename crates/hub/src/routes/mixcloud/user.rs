use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset};
use serde::Deserialize;

const HOST: &str = "https://www.mixcloud.com";
const GRAPHQL_URL: &str = "https://app.mixcloud.com/graphql";

fn parse_pub_date(s: &str) -> Option<DateTime<FixedOffset>> {
    crate::routes::util::parse_date(s)
}

#[derive(Debug, Deserialize)]
struct GraphqlResponse {
    data: serde_json::Value,
}

#[derive(Debug, Default, Deserialize)]
struct Picture {
    #[serde(default)]
    url: String,
    #[serde(default)]
    urlRoot: String,
}

#[derive(Debug, Default, Deserialize)]
struct Owner {
    #[serde(default)]
    displayName: String,
    #[serde(default)]
    username: String,
    #[serde(default)]
    url: String,
}

#[derive(Debug, Default, Deserialize)]
struct TagName {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Default, Deserialize)]
struct TagWrapper {
    #[serde(default)]
    tag: TagName,
}

#[derive(Debug, Default, Deserialize)]
struct StreamInfo {
    #[serde(default)]
    url: String,
    #[serde(default)]
    hlsUrl: String,
}

#[derive(Debug, Default, Deserialize)]
struct Cloudcast {
    #[serde(default)]
    name: String,
    #[serde(default)]
    slug: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    publishDate: String,
    #[serde(default)]
    picture: Picture,
    #[serde(default)]
    owner: Owner,
    #[serde(default)]
    streamInfo: StreamInfo,
    #[serde(default)]
    tags: Vec<TagWrapper>,
}

#[derive(Debug, Default, Deserialize)]
struct EdgeNode {
    #[serde(default)]
    cloudcast: Option<Cloudcast>,
    #[serde(default)]
    node: Option<Cloudcast>,
}

#[derive(Debug, Default, Deserialize)]
struct Edge {
    #[serde(default)]
    node: EdgeNode,
}

#[derive(Debug, Deserialize)]
struct Connection {
    #[serde(default)]
    edges: Vec<Edge>,
}

fn build_query(
    object_type: &str,
    object_fields: &str,
    username: &str,
    slug: Option<&str>,
) -> String {
    let lookup_key = format!("{}Lookup", object_type);
    let lookup_params = if let Some(s) = slug {
        format!(", slug: \"{}\"", s)
    } else {
        "".to_string()
    };

    format!(
        "{{{lookup}(lookup: {{username: \"{username}\"{params}}}) {{{fields}}}}}",
        lookup = lookup_key,
        username = username,
        params = lookup_params,
        fields = object_fields
    )
}

async fn call_api(
    object_type: &str,
    object_fields: &str,
    username: &str,
    slug: Option<&str>,
) -> captura_common::Result<serde_json::Value> {
    let client = captura_net::client_basic(None, None)
        .map_err(|e| Error::Network(format!("mixcloud client error: {}", e)))?;

    let query = build_query(object_type, object_fields, username, slug);

    let resp = client
        .post(GRAPHQL_URL)
        .header("Referer", HOST)
        .header("Content-Type", "application/json")
        .header("X-Requested-With", "XMLHttpRequest")
        .json(&serde_json::json!({ "query": query }))
        .send()
        .await
        .map_err(|e| Error::Network(format!("mixcloud graphql -> {}", e)))?;

    let body: GraphqlResponse = resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("mixcloud graphql json parse: {}", e)))?;

    let key = format!("{}Lookup", object_type);
    let value = body
        .data
        .get(&key)
        .cloned()
        .ok_or_else(|| Error::Parse(format!("mixcloud: {} not found", key)))?;
    Ok(value)
}

fn get_object_fields(ty: &str) -> (&'static str, String) {
    const CLOUDCAST_FIELDS: &str = "
      id
      slug
      name
      description
      publishDate
      picture(width: 1024, height: 1024) {
        url
      }
      owner {
        displayName
        username
        url
      }
      streamInfo {
        url
        hlsUrl
      }
      tags {
        tag {
          name
        }
      }
    ";

    match ty {
        "playlist" => {
            let fields = format!(
                "
        name
        description
        picture {{
          urlRoot
        }}
        items(first: 100) {{
          edges {{
            node {{
              cloudcast {{
                {fields}
              }}
            }}
          }}
        }}
      ",
                fields = CLOUDCAST_FIELDS
            );
            ("playlist", fields)
        }
        _ => {
            let node_template = if ty == "listens" {
                format!("node {{ cloudcast {{ {} }} }}", CLOUDCAST_FIELDS)
            } else {
                format!("node {{ {} }}", CLOUDCAST_FIELDS)
            };

            let field_name = match ty {
                "uploads" => "uploads",
                "reposts" => "reposted",
                "favorites" => "favorites",
                "listens" => "listeningHistory",
                "stream" => "stream",
                _ => "uploads",
            };

            let fields = format!(
                "
        displayName
        biog
        picture {{
          urlRoot
        }}
        {field}(first: 100) {{
          edges {{
            {node}
          }}
        }}
      ",
                field = field_name,
                node = node_template
            );
            ("user", fields)
        }
    }
}

fn extract_edges(value: &serde_json::Value, ty: &str) -> Vec<Cloudcast> {
    let field_name = match ty {
        "uploads" => "uploads",
        "reposts" => "reposted",
        "favorites" => "favorites",
        "listens" => "listeningHistory",
        "stream" => "stream",
        "playlist" => "items",
        _ => "uploads",
    };

    if ty == "playlist" {
        if let Some(items) = value.get(field_name) {
            if let Ok(conn) = serde_json::from_value::<Connection>(items.clone()) {
                let mut out = Vec::new();
                for edge in conn.edges {
                    if let Some(cc) = edge.node.cloudcast {
                        out.push(cc);
                    }
                }
                return out;
            }
        }
    } else if let Some(conn_val) = value.get(field_name) {
        if let Ok(conn) = serde_json::from_value::<Connection>(conn_val.clone()) {
            let mut out = Vec::new();
            for edge in conn.edges {
                if let Some(cc) = edge.node.node {
                    out.push(cc);
                } else if let Some(cc) = edge.node.cloudcast {
                    out.push(cc);
                }
            }
            return out;
        }
    }

    Vec::new()
}

fn playlist_title(display_name: &str, ty: &str, playlist_name: Option<&str>) -> String {
    let type_name = match ty {
        "uploads" => "Shows",
        "reposts" => "Reposts",
        "favorites" => "Favorites",
        "listens" => "History",
        "stream" => "Stream",
        "playlist" => "Playlist",
        _ => ty,
    };
    if ty == "playlist" {
        if let Some(name) = playlist_name {
            return format!("Mixcloud - {}'s Playlist: {}", display_name, name);
        }
        return format!("Mixcloud - {}'s Playlist", display_name);
    }
    format!("Mixcloud - {}'s {}", display_name, type_name)
}

pub const META_MIXCLOUD_USER: RouteMeta = RouteMeta {
    hub_id: "mixcloud/user",
    path: "/mixcloud/:username/:type?",
    categories: &["multimedia"],
    example: "/mixcloud/dholbach/uploads",
    params: &[
        ParamMeta {
            name: "username",
            description: "Mixcloud username from profile URL.",
            default: None,
            options: &[],
        },
        ParamMeta {
            name: "type",
            description:
                "Feed type: uploads (default), reposts, favorites, listens, stream, or playlist (with playlist slug).",
            default: Some("uploads"),
            options: &[
                ("uploads", "Shows"),
                ("reposts", "Reposts"),
                ("favorites", "Favorites"),
                ("listens", "History"),
                ("stream", "Stream"),
                ("playlist", "Playlist items"),
            ],
        },
    ],
    features: Features {
        require_config: &[],
        require_puppeteer: false,
        anti_crawler: false,
        support_bt: false,
        support_podcast: true,
        support_scihub: false,
        nsfw: false,
    },
    radar: &[
        Radar {
            source: &["mixcloud.com/:username/:type?", "www.mixcloud.com/:username/:type?"],
            target: "/:username/:type?",
        },
        Radar {
            source: &["mixcloud.com/:username/playlists/:playlist", "www.mixcloud.com/:username/playlists/:playlist"],
            target: "/:username/playlist",
        },
    ],
    name: "Mixcloud User / Playlist",
    maintainers: &["captura"],
    url: "https://www.mixcloud.com",
    description:
        "Mixcloud user uploads, favorites, reposts, history and playlists, aligned with RSSHub /mixcloud routes.",
    default_view: Some("podcast"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let username = ctx
        .param_str("username")
        .ok_or_else(|| Error::Config("mixcloud: missing username parameter".to_string()))?;
    let playlist_slug = ctx.param_str("playlist");
    let mut ty = ctx.param_str("type").unwrap_or("uploads");

    if playlist_slug.is_some() {
        ty = "playlist";
    }

    let limit = ctx.param_i64("limit").unwrap_or(50).max(1) as usize;

    let (object_type, object_fields) = get_object_fields(ty);
    let data = call_api(object_type, &object_fields, username, playlist_slug).await?;

    let (display_name, desc, picture_root) = if ty == "playlist" {
        let name = username.to_string();
        let description = data
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let picture_root = data
            .get("picture")
            .and_then(|v| v.get("urlRoot"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        (name, description, picture_root)
    } else {
        let display_name = data
            .get("displayName")
            .and_then(|v| v.as_str())
            .unwrap_or(username)
            .to_string();
        let description = data
            .get("biog")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let picture_root = data
            .get("picture")
            .and_then(|v| v.get("urlRoot"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        (display_name, description, picture_root)
    };

    let image = if picture_root.is_empty() {
        None
    } else {
        Some(format!(
            "https://thumbnailer.mixcloud.com/unsafe/480x480/{}",
            picture_root
        ))
    };

    let cloudcasts = extract_edges(&data, ty);
    let mut items = Vec::new();

    for cc in cloudcasts.into_iter().take(limit) {
        let title = cc.name.clone();
        if title.is_empty() {
            continue;
        }

        let link = format!(
            "{}/{}/{}",
            HOST,
            cc.owner.username.trim_start_matches('/'),
            cc.slug
        );

        let pub_date = parse_pub_date(&cc.publishDate);

        let mut description = String::new();
        if !cc.description.is_empty() {
            description.push_str(&cc.description);
        }
        if !cc.picture.url.is_empty() {
            if !description.is_empty() {
                description.push_str("<br>");
            }
            description.push_str(&format!(
                "<p><img src=\"{src}\" alt=\"{alt}\"></p>",
                src = cc.picture.url,
                alt = cc.name
            ));
        }

        let mut categories = Vec::new();
        for tw in &cc.tags {
            if !tw.tag.name.is_empty() {
                categories.push(tw.tag.name.clone());
            }
        }

        let enclosure_url = if !cc.streamInfo.hlsUrl.is_empty() {
            Some(cc.streamInfo.hlsUrl.clone())
        } else if !cc.streamInfo.url.is_empty() {
            Some(cc.streamInfo.url.clone())
        } else {
            None
        };

        items.push(HubItem {
            title,
            description: if description.is_empty() {
                None
            } else {
                Some(description)
            },
            link: Some(link),
            author: Some(cc.owner.displayName.clone()),
            pub_date,
            categories,
        });
    }

    let playlist_name = data
        .get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let feed_title = playlist_title(&display_name, ty, playlist_name.as_deref());
    let feed_link = if ty == "playlist" {
        if let Some(slug) = playlist_slug {
            format!("{}/{}/playlists/{}/", HOST, username, slug)
        } else {
            format!("{}/{}/playlists/", HOST, username)
        }
    } else {
        let path: String = if ty == "uploads" {
            String::new()
        } else {
            format!("{}/", ty)
        };
        format!("{}/{}/{}", HOST, username, path)
    };

    Ok(HubData {
        title: feed_title,
        description: Some(desc),
        link: Some(feed_link),
        image,
        language: Some("en".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_MIXCLOUD_USER: Route = Route {
    meta: &META_MIXCLOUD_USER,
    handler: handler_fn,
};
