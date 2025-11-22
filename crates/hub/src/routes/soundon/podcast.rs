use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;
use chrono::{DateTime, FixedOffset};
use serde::Deserialize;

const API_BASE: &str = "https://api.soundon.fm/v2/client";

fn parse_pub_date(raw: &str) -> Option<DateTime<FixedOffset>> {
    crate::routes::util::parse_date(raw)
}

#[derive(Debug, Deserialize)]
struct PodcastInfo {
    title: String,
    description: String,
    #[serde(rename = "artistName")]
    artist_name: String,
    #[serde(rename = "itunesCategories")]
    itunes_categories: Vec<String>,
    explicit: bool,
    cover: String,
    language: String,
    url: String,
}

#[derive(Debug, Deserialize)]
struct EpisodeWrapper {
    data: Episode,
}

#[derive(Debug, Deserialize)]
struct Episode {
    title: String,
    #[serde(rename = "contentEncoded")]
    content_encoded: String,
    url: String,
    #[serde(rename = "artistName")]
    artist_name: String,
    #[serde(rename = "publishDate")]
    publish_date: String,
    cover: String,
    #[serde(rename = "audioUrl")]
    audio_url: Option<String>,
    #[serde(rename = "audioType")]
    audio_type: Option<String>,
    duration: Option<i64>,
    #[serde(rename = "itunesKeywords")]
    itunes_keywords: Option<Vec<String>>,
}

pub const META_SOUNDON_PODCAST: RouteMeta = RouteMeta {
    hub_id: "soundon/podcast",
    path: "/soundon/podcast/:id",
    categories: &["multimedia"],
    example: "/soundon/podcast/33a68cdc-18ad-4192-84cc-22bd7fdc6a31",
    params: &[
        ParamMeta {
            name: "id",
            description: "SoundOn Podcast ID，可从 player.soundon.fm/p/:id URL 中获取。",
            default: None,
            options: &[],
        },
        ParamMeta {
            name: "limit",
            description: "最大单集数量（默认 30）。",
            default: Some("30"),
            options: &[],
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
    radar: &[Radar {
        source: &["player.soundon.fm/p/:id"],
        target: "/podcast/:id",
    }],
    name: "SoundOn 播客",
    maintainers: &["captura"],
    url: "https://player.soundon.fm",
    description:
        "SoundOn 官方播客节目，基于公开 JSON API，包含音频播放链接（通过 HTML5 audio 内嵌）。",
    default_view: Some("podcast"),
};

async fn fetch_podcast_info(id: &str) -> Result<PodcastInfo> {
    #[derive(Debug, Deserialize)]
    struct Inner<T> {
        data: T,
    }
    #[derive(Debug, Deserialize)]
    struct Outer<T> {
        data: T,
    }

    let client = client_basic(None, None).map_err(|e| Error::Network(e.to_string()))?;
    let url = format!("{API_BASE}/podcasts/{id}");
    let resp = client
        .get(&url)
        .header("api-token", "KilpEMLQeNzxmNBL55u5")
        .send()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "soundon podcast info: {} -> http status {}",
            url, status
        )));
    }
    let outer: Outer<Inner<PodcastInfo>> = resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("soundon podcast info json: {e}")))?;
    Ok(outer.data.data)
}

async fn fetch_episodes(id: &str) -> Result<Vec<Episode>> {
    #[derive(Debug, Deserialize)]
    struct Outer<T> {
        data: T,
    }

    let client = client_basic(None, None).map_err(|e| Error::Network(e.to_string()))?;
    let url = format!("{API_BASE}/podcasts/{id}/episodes");
    let resp = client
        .get(&url)
        .header("api-token", "KilpEMLQeNzxmNBL55u5")
        .send()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "soundon episodes: {} -> http status {}",
            url, status
        )));
    }
    let outer: Outer<Vec<EpisodeWrapper>> = resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("soundon episodes json: {e}")))?;
    Ok(outer.data.into_iter().map(|w| w.data).collect())
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let id = ctx.param_str("id").ok_or_else(|| {
        captura_common::Error::Parse("soundon/podcast: id is required".to_string())
    })?;
    let limit = ctx.param_i64("limit").unwrap_or(30).max(1) as usize;

    let info = fetch_podcast_info(id).await?;
    let episodes = fetch_episodes(id).await?;

    let mut items = Vec::new();

    for ep in episodes.into_iter().take(limit) {
        let title = ep.title.trim().to_string();
        if title.is_empty() {
            continue;
        }

        let link = Some(ep.url.clone());
        let pub_date = parse_pub_date(&ep.publish_date);

        let mut desc = String::new();

        if let Some(audio_url) = ep.audio_url.as_ref() {
            if !audio_url.is_empty() {
                desc.push_str(&format!(
                    "<p><audio controls src=\"{src}\">Your browser does not support the audio element.</audio></p>",
                    src = audio_url
                ));
            }
        }

        if !ep.cover.is_empty() {
            desc.push_str(&format!(
                "<p><img src=\"{src}\" alt=\"{alt}\"></p>",
                src = ep.cover,
                alt = title
            ));
        }

        if !ep.content_encoded.trim().is_empty() {
            desc.push_str(&ep.content_encoded);
        }

        let mut categories = Vec::new();
        if let Some(keys) = ep.itunes_keywords.as_ref() {
            for k in keys {
                let t = k.trim();
                if !t.is_empty() && !categories.contains(&t.to_string()) {
                    categories.push(t.to_string());
                }
            }
        }
        if !categories.iter().any(|c| c.eq_ignore_ascii_case("podcast")) {
            categories.push("podcast".to_string());
        }

        items.push(HubItem {
            title,
            description: if desc.is_empty() { None } else { Some(desc) },
            link,
            author: Some(ep.artist_name.clone()),
            pub_date,
            categories,
        });
    }

    let category_str = if info.itunes_categories.is_empty() {
        None
    } else {
        Some(info.itunes_categories.join(", "))
    };

    let mut description = info.description.clone();
    if let Some(cat) = &category_str {
        if !cat.is_empty() {
            description.push_str(&format!("\n\nCategories: {}", cat));
        }
    }

    Ok(HubData {
        title: info.title.clone(),
        description: if description.is_empty() {
            None
        } else {
            Some(description)
        },
        link: Some(info.url.clone()),
        image: Some(info.cover.clone()),
        language: Some(info.language.clone()),
        items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_SOUNDON_PODCAST: Route = Route {
    meta: &META_SOUNDON_PODCAST,
    handler: handler_fn,
};
