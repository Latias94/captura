use crate::routes::types::{
    FeatureConfig, Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;
use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeZone, Utc};
use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;

const ROOT_URL: &str = "https://www.qingting.fm";
const API_BASE: &str = "https://i.qingting.fm/capi";

fn parse_pub_date(raw: &str) -> Option<DateTime<FixedOffset>> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    if let Ok(naive) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        if let Some(offset) = FixedOffset::east_opt(8 * 3600) {
            return Some(offset.from_utc_datetime(&naive));
        }
    }
    crate::routes::util::parse_date(raw)
}

fn get_qingting_id() -> String {
    std::env::var("QINGTING_ID")
        .ok()
        .unwrap_or_default()
        .trim()
        .to_string()
}

type HmacMd5 = hmac::Hmac<md5::Md5>;

fn sign_path(path: &str) -> Result<String> {
    use hmac::Mac;
    let mut mac = HmacMd5::new_from_slice(b"fpMn12&38f_2e")
        .map_err(|e| Error::Parse(format!("qingting: hmac init failed: {e}")))?;
    mac.update(path.as_bytes());
    let bytes = mac.finalize().into_bytes();
    Ok(hex::encode(bytes))
}

fn build_media_url(channel_id: &str, program_id: i64, qingting_id: &str) -> Result<String> {
    let t = Utc::now().timestamp_millis();
    let path = format!(
        "/audiostream/redirect/{}/{}?access_token=&device_id=MOBILESITE&qingting_id={}&t={}",
        channel_id, program_id, qingting_id, t
    );
    let sign = sign_path(&path)?;
    Ok(format!("https://audio.qingting.fm{}&sign={}", path, sign))
}

#[derive(Debug, Deserialize)]
struct ApiWrapper<T> {
    data: T,
}

#[derive(Debug, Deserialize)]
struct Channel {
    id: i64,
    v: String,
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    cover: String,
    #[serde(default)]
    thumbs: HashMap<String, String>,
    #[serde(default)]
    podcasters: Vec<Podcaster>,
    #[serde(default)]
    purchase: Option<Purchase>,
    #[serde(default)]
    user_relevance: Option<UserRelevance>,
}

#[derive(Debug, Deserialize)]
struct Podcaster {
    #[serde(default)]
    nick_name: String,
}

#[derive(Debug, Deserialize)]
struct Purchase {
    #[serde(default)]
    item_type: i32,
}

#[derive(Debug, Deserialize)]
struct UserRelevance {
    #[serde(default)]
    sale_status: String,
}

#[derive(Debug, Deserialize)]
struct ProgramsData {
    programs: Vec<Program>,
}

#[derive(Debug, Deserialize)]
struct Program {
    id: i64,
    title: String,
    #[serde(default)]
    cover: String,
    #[serde(default)]
    duration: i64,
    #[serde(default)]
    update_time: String,
    #[serde(default)]
    isfree: bool,
}

pub const META_QINGTING_PODCAST: RouteMeta = RouteMeta {
    hub_id: "qingting/podcast",
    path: "/qingting/podcast/:id",
    categories: &["multimedia"],
    example: "/qingting/podcast/293411",
    params: &[
        ParamMeta {
            name: "id",
            description:
                "专辑 ID，可在蜻蜓 FM 专辑页 URL 中找到，例如 https://www.qingting.fm/channels/293411。",
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
        require_config: &[FeatureConfig {
            name: "QINGTING_ID",
            description: "可选，蜻蜓 FM 用户 ID。部分专辑需要会员身份，登录网页端后在控制台运行 JSON.parse(localStorage.getItem(\"user\")).qingting_id 获取。",
            optional: true,
        }],
        require_puppeteer: false,
        anti_crawler: false,
        support_bt: false,
        support_podcast: true,
        support_scihub: false,
        nsfw: false,
    },
    radar: &[Radar {
        source: &["qingting.fm/channels/:id"],
        target: "/podcast/:id",
    }],
    name: "蜻蜓 FM 播客",
    maintainers: &["captura"],
    url: "https://www.qingting.fm",
    description:
        "蜻蜓 FM 专辑播客，对标 RSSHub /qingting/podcast/:id，使用公开 JSON API 与页面内节目详情。",
    default_view: Some("podcast"),
};

async fn fetch_channel(channel_id: &str) -> Result<Channel> {
    let url = format!("{}/v3/channel/{}", API_BASE, channel_id);
    let client = client_basic(None, None).map_err(|e| Error::Network(e.to_string()))?;
    let resp = client
        .get(&url)
        .header("Referer", ROOT_URL)
        .send()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "qingting channel: {} -> http status {}",
            url, status
        )));
    }
    let wrapper: ApiWrapper<Channel> = resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("qingting channel json: {e}")))?;
    Ok(wrapper.data)
}

async fn fetch_channel_with_user(channel_id: &str, qingting_id: &str) -> Result<Option<Channel>> {
    if qingting_id.trim().is_empty() {
        return Ok(None);
    }
    let url = format!(
        "{}/v3/channel/{}?user_id={}",
        API_BASE, channel_id, qingting_id
    );
    let client = client_basic(None, None).map_err(|e| Error::Network(e.to_string()))?;
    let resp = client
        .get(&url)
        .header("Referer", ROOT_URL)
        .send()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;
    let status = resp.status();
    if !status.is_success() {
        // 配置错误时不中断主流程，只记录成解析异常。
        return Ok(None);
    }
    let wrapper: ApiWrapper<Channel> = resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("qingting channel(user) json: {e}")))?;
    Ok(Some(wrapper.data))
}

async fn fetch_programs(channel_id: &str, version: &str, limit: usize) -> Result<Vec<Program>> {
    let page_size = limit.max(1);
    let url = format!(
        "{}/channel/{}/programs/{}?curpage=1&pagesize={}&order=asc",
        API_BASE, channel_id, version, page_size
    );
    let client = client_basic(None, None).map_err(|e| Error::Network(e.to_string()))?;
    let resp = client
        .get(&url)
        .header("Referer", ROOT_URL)
        .send()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "qingting programs: {} -> http status {}",
            url, status
        )));
    }
    let wrapper: ApiWrapper<ProgramsData> = resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("qingting programs json: {e}")))?;
    Ok(wrapper.data.programs)
}

async fn fetch_program_richtext(channel_id: &str, program_id: i64) -> Result<Option<String>> {
    let url = format!(
        "{}/channels/{}/programs/{}/",
        ROOT_URL, channel_id, program_id
    );
    let client = client_basic(None, None).map_err(|e| Error::Network(e.to_string()))?;
    let resp = client
        .get(&url)
        .header("Referer", ROOT_URL)
        .send()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;
    let status = resp.status();
    if !status.is_success() {
        return Ok(None);
    }
    let html = resp
        .text()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;

    let re = Regex::new(r#"(?s)},"program":(.*?),"plist":"#)
        .map_err(|e| Error::Parse(format!("qingting regex error: {e}")))?;
    let caps = match re.captures(&html) {
        Some(c) => c,
        None => return Ok(None),
    };
    let json_str = caps.get(1).map(|m| m.as_str()).unwrap_or("").trim();
    if json_str.is_empty() {
        return Ok(None);
    }
    let value: serde_json::Value = serde_json::from_str(json_str)
        .map_err(|e| Error::Parse(format!("qingting program detail json: {e}")))?;
    let richtext = value
        .get("richtext")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if richtext.is_empty() {
        Ok(None)
    } else {
        Ok(Some(richtext))
    }
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let channel_id = ctx.param_str("id").ok_or_else(|| {
        captura_common::Error::Parse("qingting/podcast: id is required".to_string())
    })?;
    let limit = ctx.param_i64("limit").unwrap_or(30).max(1) as usize;

    let base_channel = fetch_channel(channel_id).await?;
    let qingting_id = get_qingting_id();
    let user_channel = fetch_channel_with_user(channel_id, &qingting_id).await?;
    let effective_channel = user_channel.as_ref().unwrap_or(&base_channel);

    let is_charged = effective_channel
        .purchase
        .as_ref()
        .map(|p| p.item_type != 0)
        .unwrap_or(false);
    let is_paid = effective_channel
        .user_relevance
        .as_ref()
        .map(|u| u.sale_status.as_str() == "paid")
        .unwrap_or(false);

    let programs = fetch_programs(channel_id, &base_channel.v, limit).await?;

    let authors = if effective_channel.podcasters.is_empty() {
        None
    } else {
        Some(
            effective_channel
                .podcasters
                .iter()
                .map(|p| p.nick_name.trim())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(","),
        )
    };

    let channel_img = effective_channel
        .thumbs
        .get("400_thumb")
        .cloned()
        .or_else(|| effective_channel.thumbs.get("200_thumb").cloned())
        .or_else(|| effective_channel.thumbs.get("800_thumb").cloned())
        .or_else(|| {
            if !effective_channel.cover.is_empty() {
                Some(effective_channel.cover.clone())
            } else {
                None
            }
        });

    let mut items = Vec::new();

    for program in programs.into_iter().take(limit) {
        let title = program.title.trim().to_string();
        if title.is_empty() {
            continue;
        }

        let link = format!(
            "{}/channels/{}/programs/{}/",
            ROOT_URL, channel_id, program.id
        );

        let pub_date = parse_pub_date(&program.update_time);

        let mut desc = String::new();

        if !is_charged || is_paid || program.isfree {
            if let Ok(audio_url) = build_media_url(channel_id, program.id, &qingting_id) {
                desc.push_str("<p>");
                desc.push_str(&crate::routes::util::html_audio(&audio_url));
                desc.push_str("</p>");
            }
        }

        if !program.cover.is_empty() {
            desc.push_str("<p>");
            desc.push_str(&crate::routes::util::html_img(&program.cover, &title));
            desc.push_str("</p>");
        }

        if let Ok(Some(richtext)) = fetch_program_richtext(channel_id, program.id).await {
            desc.push_str(&richtext);
        } else if desc.is_empty() {
            desc.push_str(&format!(
                r#"<p><a href="{link}">在蜻蜓 FM 查看节目详情</a></p>"#,
                link = link
            ));
        }

        let mut categories = Vec::new();
        categories.push("podcast".to_string());
        categories.push("qingting".to_string());

        items.push(HubItem {
            title,
            description: if desc.is_empty() { None } else { Some(desc) },
            link: Some(link),
            author: authors.clone(),
            pub_date,
            categories,
        });
    }

    Ok(HubData {
        title: format!("{} - 蜻蜓FM", effective_channel.title),
        description: if effective_channel.description.is_empty() {
            None
        } else {
            Some(effective_channel.description.clone())
        },
        link: Some(format!("{}/channels/{}/", ROOT_URL, channel_id)),
        image: channel_img,
        language: Some("zh-CN".to_string()),
        items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_QINGTING_PODCAST: Route = Route {
    meta: &META_QINGTING_PODCAST,
    handler: handler_fn,
};
