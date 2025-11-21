use crate::routes::types::{
    FeatureConfig, Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;
use chrono::{DateTime, FixedOffset};
use scraper::{Html, Selector};
use serde::Deserialize;

pub const META_YOUTUBE_LIVE: RouteMeta = RouteMeta {
    hub_id: "youtube/live",
    path: "/youtube/live/:username",
    categories: &["live"],
    example: "/youtube/live/@GawrGura",
    params: &[ParamMeta {
        name: "username",
        description: "频道路径，例如 @handle、channel/UC...、c/xxx 等。",
        default: None,
        options: &[],
    }],
    features: Features::with_config(&[FeatureConfig {
        name: "YOUTUBE_API_KEY",
        description: "YouTube Data API Key，用于查询频道与直播状态（对应 RSSHub 的 YOUTUBE_KEY）。",
        optional: false,
    }]),
    radar: &[Radar {
        source: &["www.youtube.com/:username"],
        target: "/live/:username",
    }],
    name: "YouTube Live",
    maintainers: &["captura"],
    url: "https://www.youtube.com",
    description: "YouTube 指定频道的直播状态，对标 RSSHub /youtube/live/:username 的简化实现。",
    default_view: Some("notifications"),
};

#[derive(Debug, Deserialize)]
struct YtSearchResp {
    items: Vec<YtSearchItem>,
}

#[derive(Debug, Deserialize)]
struct YtSearchItem {
    id: YtSearchId,
    snippet: YtSnippet,
}

#[derive(Debug, Deserialize)]
struct YtSearchId {
    #[serde(rename = "videoId")]
    video_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct YtSnippet {
    title: String,
    description: String,
    #[serde(rename = "publishedAt")]
    published_at: String,
}

fn parse_date(raw: &str) -> Option<DateTime<FixedOffset>> {
    crate::routes::util::parse_date(raw)
}

async fn fetch_channel_id(username: &str) -> Result<(String, String)> {
    let url = format!("https://www.youtube.com/{}", username);
    let html = crate::routes::util::get_html(&url).await?;
    let doc = Html::parse_document(&html);
    let sel_id = Selector::parse("meta[itemprop=\"identifier\"]").unwrap();
    let sel_name = Selector::parse("meta[itemprop=\"name\"]").unwrap();

    let channel_id = doc
        .select(&sel_id)
        .next()
        .and_then(|el| el.value().attr("content"))
        .map(|s| s.to_string());
    let channel_name = doc
        .select(&sel_name)
        .next()
        .and_then(|el| el.value().attr("content"))
        .map(|s| s.to_string())
        .unwrap_or_else(|| username.to_string());

    let channel_id = match channel_id {
        Some(id) => id,
        None => {
            // 回退：直接把 username 当作 channelId 用（适用于 /channel/UC... 的情况）。
            if username.starts_with("channel/") {
                username.trim_start_matches("channel/").to_string()
            } else {
                return Err(Error::Network("failed to resolve channel id".to_string()));
            }
        }
    };

    Ok((channel_id, channel_name))
}

async fn fetch_live_items(channel_id: &str, api_key: &str) -> Result<Vec<HubItem>> {
    let client = client_basic(None, None).map_err(|e| Error::Network(e.to_string()))?;
    let url = format!(
        "https://www.googleapis.com/youtube/v3/search?part=snippet&channelId={}&eventType=live&type=video&key={}",
        channel_id, api_key
    );
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "youtube search -> http status {}",
            status
        )));
    }
    let body: YtSearchResp = resp.json().await.map_err(|e| Error::Parse(e.to_string()))?;

    let mut items = Vec::new();
    for item in body.items {
        let video_id = match item.id.video_id {
            Some(id) => id,
            None => continue,
        };
        let link = format!("https://www.youtube.com/watch?v={}", video_id);
        let pub_date = parse_date(&item.snippet.published_at);

        items.push(HubItem {
            title: item.snippet.title.clone(),
            description: Some(item.snippet.description.clone()),
            link: Some(link),
            author: None,
            pub_date,
            categories: Vec::new(),
        });
    }

    Ok(items)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let username = ctx
        .param_str("username")
        .ok_or_else(|| Error::Config("missing youtube username param".to_string()))?;
    let api_key = std::env::var("YOUTUBE_API_KEY")
        .map_err(|_| Error::Config("YOUTUBE_API_KEY is required".to_string()))?;

    let (channel_id, channel_name) = fetch_channel_id(username).await?;
    let items = fetch_live_items(&channel_id, &api_key)
        .await
        .unwrap_or_default();

    Ok(HubData {
        title: format!("{}'s Live Status", channel_name),
        description: Some(format!("{} 的直播状态。", channel_name)),
        link: Some(format!("https://www.youtube.com/channel/{}", channel_id)),
        image: None,
        language: Some("en".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_YOUTUBE_LIVE: Route = Route {
    meta: &META_YOUTUBE_LIVE,
    handler: handler_fn,
};
