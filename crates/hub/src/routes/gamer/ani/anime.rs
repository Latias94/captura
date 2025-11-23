use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;

#[derive(Debug, serde::Deserialize)]
struct AnimeResponse {
    error: Option<AnimeError>,
    data: Option<AnimeData>,
}

#[derive(Debug, serde::Deserialize)]
struct AnimeError {
    message: String,
}

#[derive(Debug, serde::Deserialize)]
struct AnimeData {
    anime: AnimeInfo,
}

#[derive(Debug, serde::Deserialize)]
struct AnimeInfo {
    title: String,
    content: Option<String>,
    #[serde(rename = "anime_sn")]
    anime_sn: i64,
    volumes: Vec<Vec<AnimeVolume>>,
}

#[derive(Debug, serde::Deserialize)]
struct AnimeVolume {
    volume: i64,
    cover: String,
    #[serde(rename = "video_sn")]
    video_sn: i64,
}

pub const META_GAMER_ANI_ANIME: RouteMeta = RouteMeta {
    hub_id: "gamer/ani/anime",
    path: "/gamer/ani/anime/:sn",
    categories: &["anime"],
    example: "/gamer/ani/anime/36868",
    params: &[ParamMeta {
        name: "sn",
        description: "Anime sn from ani.gamer.com.tw URLs.",
        default: None,
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["ani.gamer.com.tw"],
        target: "/anime/:sn",
    }],
    name: "動畫瘋 - 動畫",
    maintainers: &["captura"],
    url: "https://ani.gamer.com.tw",
    description: "Bahamut Ani-Gamer series episodes, aligned with RSSHub /gamer/ani/anime/:sn route.",
    default_view: Some("videos"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let sn = ctx
        .param_str("sn")
        .ok_or_else(|| Error::Config("gamer/ani/anime: missing sn parameter".to_string()))?;

    let client = client_basic(None, None)
        .map_err(|e| Error::Network(format!("gamer/ani/anime client error: {}", e)))?;
    let resp = client
        .get("https://api.gamer.com.tw/mobile_app/anime/v3/video.php")
        .query(&[("sn", sn)])
        .send()
        .await
        .map_err(|e| Error::Network(format!("gamer/ani/anime: {}", e)))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "gamer/ani/anime: http status {}",
            status
        )));
    }
    let parsed: AnimeResponse = resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("gamer/ani/anime parse error: {}", e)))?;

    if let Some(err) = parsed.error {
        return Err(Error::Network(format!(
            "gamer/ani/anime api error: {}",
            err.message
        )));
    }

    let data = parsed
        .data
        .ok_or_else(|| Error::Parse("gamer/ani/anime: missing data".to_string()))?;
    let anime = data.anime;

    let base_title = anime
        .title
        .replace(|c: char| c == '[' || c == ']', "")
        .trim()
        .to_string();

    let mut items = Vec::new();
    if let Some(first_volumes) = anime.volumes.get(0) {
        for v in first_volumes {
            let title = format!("{} 第 {} 集", base_title, v.volume);
            let description = format!(r#"<img src="{}">"#, v.cover);
            let link = format!("https://ani.gamer.com.tw/animeVideo.php?sn={}", v.video_sn);
            items.push(HubItem {
                title,
                description: Some(description),
                link: Some(link),
                author: None,
                pub_date: None,
                categories: Vec::new(),
            });
        }
        items.reverse();
    }

    Ok(HubData {
        title: base_title.clone(),
        description: anime.content.map(|c| c.trim().to_string()),
        link: Some(format!(
            "https://ani.gamer.com.tw/animeRef.php?sn={}",
            anime.anime_sn
        )),
        image: None,
        language: Some("zh-TW".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_GAMER_ANI_ANIME: Route = Route {
    meta: &META_GAMER_ANI_ANIME,
    handler: handler_fn,
};
