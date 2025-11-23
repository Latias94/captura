use crate::routes::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use crate::routes::util;
use captura_hub_macros::register_hub_route;
use serde::Deserialize;
use std::collections::HashMap;

const PLUGINS_URL: &str = "https://raw.githubusercontent.com/obsidianmd/obsidian-releases/refs/heads/master/community-plugins.json";
const STATS_URL: &str = "https://raw.githubusercontent.com/obsidianmd/obsidian-releases/HEAD/community-plugin-stats.json";

#[derive(Debug, Deserialize)]
struct Plugin {
    id: String,
    name: String,
    author: String,
    description: String,
    repo: String,
}

#[derive(Debug, Deserialize)]
struct PluginStatsEntry {
    downloads: Option<u64>,
    updated: Option<i64>,
    #[serde(flatten)]
    _extra: HashMap<String, serde_json::Value>,
}

pub const META_OBSIDIAN_PLUGINS: RouteMeta = RouteMeta {
    hub_id: "obsidian/plugins",
    path: "/obsidian/plugins",
    categories: &["program-update"],
    example: "/obsidian/plugins",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["obsidian.md/plugins"],
        target: "/plugins",
    }],
    name: "Obsidian Plugins",
    maintainers: &["captura"],
    url: "https://obsidian.md/plugins",
    description: "Community plugins from the official Obsidian plugin registry, with download stats from GitHub.",
    default_view: Some("articles"),
};

pub async fn handler(_ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let plugins: Vec<Plugin> = util::get_json(PLUGINS_URL).await?;
    let stats_map: HashMap<String, PluginStatsEntry> = util::get_json(STATS_URL).await?;

    let mut items = Vec::new();

    for p in plugins {
        let stats = stats_map.get(&p.id);
        let downloads = stats.and_then(|s| s.downloads).unwrap_or(0);
        let updated_ms = stats.and_then(|s| s.updated);
        let pub_date = updated_ms.and_then(|ms| util::parse_ms_timestamp(ms, 0));

        let mut description = p.description.clone();
        description.push_str("<br><br>Downloads: ");
        description.push_str(&downloads.to_string());

        let link = format!("https://github.com/{}", p.repo);

        items.push(HubItem {
            title: p.name.clone(),
            description: Some(description),
            link: Some(link),
            author: Some(p.author.clone()),
            pub_date,
            categories: Vec::new(),
        });
    }

    Ok(HubData {
        title: "Obsidian Plugins".to_string(),
        description: Some(
            "Community plugins for Obsidian, based on the official plugin registry.".to_string(),
        ),
        link: Some("https://obsidian.md/plugins".to_string()),
        image: None,
        language: Some("en-US".to_string()),
        items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_OBSIDIAN_PLUGINS: Route = Route {
    meta: &META_OBSIDIAN_PLUGINS,
    handler: handler_fn,
};
