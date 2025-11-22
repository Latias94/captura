use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::Result;
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;
use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeZone};
use serde::Deserialize;
use std::collections::HashMap;

const API_URL: &str = "https://gateway-front-external.nio.com/moat/100914/v2/audio/list";
const ROOT_URL: &str = "https://www.nio.com";

fn parse_ms_ts(ts: i64) -> Option<DateTime<FixedOffset>> {
    // NIO 接口返回毫秒级时间戳，采用中国时区（+8）。
    let secs = ts / 1000;
    let nsecs = ((ts % 1000) * 1_000_000).max(0) as u32;
    let naive = NaiveDateTime::from_timestamp_opt(secs, nsecs)?;
    let offset = FixedOffset::east_opt(8 * 3600)?;
    Some(offset.from_utc_datetime(&naive))
}

#[derive(Debug, Deserialize)]
struct NioListResp {
    result: NioResult,
}

#[derive(Debug, Deserialize)]
struct NioResult {
    #[serde(rename = "dataList")]
    data_list: Vec<NioAudio>,
}

#[derive(Debug, Deserialize)]
struct NioAudio {
    #[serde(rename = "albumId")]
    album_id: i64,
    #[serde(rename = "albumName")]
    album_name: String,
    #[serde(rename = "albumPic")]
    album_pic: String,
    #[serde(rename = "albumDesc")]
    album_desc: String,
    #[serde(rename = "audioId")]
    audio_id: i64,
    #[serde(rename = "audioName")]
    audio_name: String,
    #[serde(rename = "audioDes")]
    audio_des: Option<String>,
    #[serde(rename = "host")]
    host: Vec<String>,
    #[serde(rename = "duration")]
    duration_ms: i64,
    #[serde(rename = "onlineTime")]
    online_time: i64,
    #[serde(rename = "aacPlayUrl192")]
    aac_play_url_192: Option<String>,
    #[serde(rename = "aacFileSize192")]
    aac_file_size_192: Option<i64>,
}

pub const META_NIO_NIORADIO: RouteMeta = RouteMeta {
    hub_id: "nio/nioradio",
    path: "/nio/nioradio/:albumid",
    categories: &["multimedia"],
    example: "/nio/nioradio/5",
    params: &[
        ParamMeta {
            name: "albumid",
            description: "电台专辑 ID，例如“资讯充电站·早间版”对应的 5。",
            default: None,
            options: &[],
        },
        ParamMeta {
            name: "limit",
            description: "最大节目数量（默认 10）。",
            default: Some("10"),
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
        source: &["app.nio.com/app/radio/share/?*container_id=:albumid"],
        target: "/nioradio/:albumid",
    }],
    name: "NIO Radio",
    maintainers: &["captura"],
    url: "https://www.nio.com",
    description:
        "蔚来 NIO Radio 电台专辑节目列表，包含音频播放链接，适合将车机节目转换为播客订阅。",
    default_view: Some("podcast"),
};

async fn fetch_nioradio(albumid: &str, limit: usize) -> Result<Vec<NioAudio>> {
    let client =
        client_basic(None, None).map_err(|e| captura_common::Error::Network(e.to_string()))?;
    let mut form: HashMap<&str, String> = HashMap::new();
    form.insert("albumId", albumid.to_string());
    form.insert("sorttype", "2".to_string());
    form.insert("pagenum", "1".to_string());
    form.insert("pagesize", limit.to_string());

    let resp = client
        .post(API_URL)
        .form(&form)
        .send()
        .await
        .map_err(|e| captura_common::Error::Network(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(captura_common::Error::Network(format!(
            "nio/nioradio: {} -> http status {}",
            API_URL,
            resp.status()
        )));
    }
    let data: NioListResp = resp
        .json()
        .await
        .map_err(|e| captura_common::Error::Parse(format!("nio/nioradio: invalid json: {e}")))?;
    Ok(data.result.data_list)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let albumid = ctx.param_str("albumid").ok_or_else(|| {
        captura_common::Error::Parse("nio/nioradio: albumid is required".to_string())
    })?;
    let limit = ctx.param_i64("limit").unwrap_or(10).max(1) as usize;

    let audios = fetch_nioradio(albumid, limit).await?;
    if audios.is_empty() {
        return Ok(HubData {
            title: format!("NIO Radio - {}", albumid),
            description: Some("NIO Radio 无节目返回，可能 albumid 无效。".to_string()),
            link: Some(ROOT_URL.to_string()),
            image: None,
            language: Some("zh-CN".to_string()),
            items: Vec::new(),
            allow_empty: true,
        });
    }

    let first = &audios[0];
    let podcast_name = first.album_name.clone();
    let podcast_image = first.album_pic.clone();
    let podcast_desc = first.album_desc.clone();

    let mut items = Vec::new();
    for a in audios {
        let title = a.audio_name.trim().to_string();
        if title.is_empty() {
            continue;
        }
        let link = format!(
            "https://app.nio.com/app/radio/share/?item_type=1&item_id={}&container_id={}",
            a.audio_id, albumid
        );

        let pub_date = parse_ms_ts(a.online_time);

        let mut desc = String::new();
        if !podcast_desc.trim().is_empty() {
            desc.push_str(&format!("<p>{}</p>", podcast_desc.trim()));
        }
        if let Some(d) = a.audio_des.as_ref() {
            if !d.trim().is_empty() {
                desc.push_str(&format!("<p>{}</p>", d.trim()));
            }
        }

        if let Some(audio_url) = a.aac_play_url_192.as_ref() {
            desc.push_str(&format!(
                "<p><audio controls src=\"{src}\">Your browser does not support the audio element.</audio></p>",
                src = audio_url
            ));
        }

        items.push(HubItem {
            title,
            description: if desc.is_empty() { None } else { Some(desc) },
            link: Some(link),
            author: Some(a.host.join(", ")),
            pub_date,
            categories: vec!["NIO Radio".to_string(), "podcast".to_string()],
        });
    }

    Ok(HubData {
        title: format!("NIO Radio - {}", podcast_name),
        description: if podcast_desc.trim().is_empty() {
            None
        } else {
            Some(podcast_desc.trim().to_string())
        },
        link: Some(ROOT_URL.to_string()),
        image: Some(podcast_image),
        language: Some("zh-CN".to_string()),
        items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_NIO_NIORADIO: Route = Route {
    meta: &META_NIO_NIORADIO,
    handler: handler_fn,
};
