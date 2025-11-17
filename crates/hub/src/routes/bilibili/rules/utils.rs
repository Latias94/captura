use captura_common::{Error, Result};
use serde::Deserialize;
use std::env;

/// Bilibili-specific helper functions used by rules and Hub glue code.
///
/// These helpers mirror a subset of RSSHub's `utils.ts`, but are kept
/// lightweight and self-contained in the `rules::bilibili` module.
///
/// Normalize a cover image URL to HTTPS and strip obvious issues.
pub fn normalize_cover_url(url: &str) -> String {
    let mut u = url.trim().to_string();
    if u.is_empty() {
        return u;
    }
    if let Some(rest) = u.strip_prefix("http://") {
        u = format!("https://{}", rest);
    }
    u
}

/// Render a simple UGC-like description with optional cover image and
/// inline video iframe.
///
/// - `embed`: whether to include an inline player when `bvid` or `aid` present.
/// - `cover_url`: optional cover image URL.
/// - `summary`: textual description.
/// - `bvid`: optional Bilibili video id in BV format.
/// - `aid`: optional numeric aid (not always available in our mappings).
pub fn render_ugc_description(
    embed: bool,
    cover_url: Option<&str>,
    summary: &str,
    bvid: Option<&str>,
    aid: Option<i64>,
) -> String {
    let mut html = String::new();

    if !summary.trim().is_empty() {
        html.push_str("<p>");
        html.push_str(summary);
        html.push_str("</p>");
    }

    if let Some(raw_cover) = cover_url {
        let normalized = normalize_cover_url(raw_cover);
        if !normalized.is_empty() {
            html.push_str(&format!(
                "<p><img src=\"{}\" referrerpolicy=\"no-referrer\"></p>",
                normalized
            ));
        }
    }

    if embed {
        if let Some(b) = bvid {
            if !b.is_empty() {
                html.push_str(&format!(
                    r#"<iframe src="https://www.bilibili.com/blackboard/newplayer.html?isOutside=true&autoplay=false&danmaku=true&highQuality=true&bvid={}" frameborder="0" allowfullscreen></iframe>"#,
                    b
                ));
            }
        } else if let Some(a) = aid {
            html.push_str(&format!(
                r#"<iframe src="https://www.bilibili.com/blackboard/newplayer.html?isOutside=true&autoplay=false&danmaku=true&highQuality=true&aid={}" frameborder="0" allowfullscreen></iframe>"#,
                a
            ));
        }
    }

    html
}

/// Pick a Bilibili cookie from environment variables.
///
/// Priority:
/// 1. `BILIBILI_COOKIE` if present and non-empty.
/// 2. Any `BILIBILI_COOKIE_*` variables (e.g. `BILIBILI_COOKIE_2267573`), picking
///    the one with the lexicographically smallest key for determinism.
pub fn pick_bilibili_cookie() -> Option<String> {
    if let Ok(v) = env::var("BILIBILI_COOKIE") {
        let v = v.trim();
        if !v.is_empty() {
            return Some(v.to_string());
        }
    }

    let mut pool: Vec<(String, String)> = env::vars()
        .filter(|(k, v)| k.starts_with("BILIBILI_COOKIE_") && !v.trim().is_empty())
        .collect();
    if pool.is_empty() {
        return None;
    }
    pool.sort_by(|a, b| a.0.cmp(&b.0));
    let (_, cookie) = pool.remove(0);
    Some(cookie.trim().to_string())
}

/// Lightweight HTTP helper for Bilibili JSON APIs.
///
/// - Applies a common User-Agent (`captura/0.1`).
/// - Adds `Referer` when provided.
/// - Optionally attaches `BILIBILI_COOKIE` as Cookie header if present.
/// - Checks HTTP status and Bilibili JSON `code` field.
pub async fn bilibili_get_json(url: &str, referer: Option<&str>) -> Result<serde_json::Value> {
    // Use the shared HTTP client builder so that UA/timeout/proxy semantics
    // match the rest of the stack. Bilibili APIs are latency-sensitive but
    // we keep timeout flexible via env; callers can constrain at the request
    // level if needed.
    let client = captura_net::client_basic(Some("captura/0.1".to_string()), None)
        .map_err(|e| Error::Network(e.to_string()))?;

    let mut req = client.get(url);

    if let Some(r) = referer {
        req = req.header(reqwest::header::REFERER, r);
    }

    if let Some(cookie) = pick_bilibili_cookie() {
        req = req.header(reqwest::header::COOKIE, cookie);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "bilibili http status {} for {}",
            status, url
        )));
    }

    let v: serde_json::Value = resp.json().await.map_err(|e| Error::Parse(e.to_string()))?;

    if let Some(code) = v.get("code").and_then(|c| c.as_i64()) {
        if code != 0 {
            let msg = v
                .get("message")
                .or_else(|| v.get("msg"))
                .and_then(|m| m.as_str())
                .unwrap_or("bilibili api error");
            // -352 is commonly used for risk control / captcha.
            let detail = if code == -352 {
                format!("bilibili risk control (code {}): {}", code, msg)
            } else {
                format!("bilibili error (code {}): {}", code, msg)
            };
            return Err(Error::Network(detail));
        }
    }

    Ok(v)
}

/// Minimal bangumi media metadata used by hub handlers.
#[derive(Debug, Clone, Deserialize)]
pub struct BangumiMediaMeta {
    pub season_id: String,
    pub title: String,
    pub evaluate: Option<String>,
    pub share_url: Option<String>,
    pub cover: Option<String>,
}

/// Simplified episode metadata for bangumi seasons.
#[derive(Debug, Clone)]
pub struct BangumiEpisode {
    pub full_title: String,
    pub number: Option<String>,
    pub cover: Option<String>,
    pub share_url: String,
}

/// Fetch bangumi media metadata by media id, similar to RSSHub's `getBangumi`.
pub async fn fetch_bangumi_media(media_id: &str) -> Result<BangumiMediaMeta> {
    let url = format!(
        "https://api.bilibili.com/pgc/view/web/media?media_id={}",
        media_id
    );
    let v = bilibili_get_json(&url, None).await?;

    let result = v
        .get("result")
        .ok_or_else(|| Error::Parse("bangumi media: missing result".into()))?;

    let season_id = result
        .get("season_id")
        .and_then(|v| v.as_i64().map(|n| n.to_string()))
        .ok_or_else(|| Error::Parse("bangumi media: missing season_id".into()))?;

    let title = result
        .get("title")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .ok_or_else(|| Error::Parse("bangumi media: missing title".into()))?;

    let evaluate = result
        .get("evaluate")
        .and_then(|v| v.as_str().map(|s| s.to_string()));

    let share_url = result
        .get("share_url")
        .and_then(|v| v.as_str().map(|s| s.to_string()));

    let cover = result
        .get("cover")
        .and_then(|v| v.as_str().map(|s| s.to_string()));

    Ok(BangumiMediaMeta {
        season_id,
        title,
        evaluate,
        share_url,
        cover,
    })
}

/// Fetch bangumi season episodes (main_section + all sections) and flatten.
pub async fn fetch_bangumi_episodes(season_id: &str) -> Result<Vec<BangumiEpisode>> {
    let url = format!(
        "https://api.bilibili.com/pgc/web/season/section?season_id={}",
        season_id
    );
    let v = bilibili_get_json(&url, None).await?;

    let result = v
        .get("result")
        .ok_or_else(|| Error::Parse("bangumi season: missing result".into()))?;

    let mut episodes = Vec::new();

    // Helper to extract episodes from an array node.
    fn extract_from_array(arr: &serde_json::Value, season_id: &str, out: &mut Vec<BangumiEpisode>) {
        if let Some(list) = arr.as_array() {
            for ep in list {
                let ep_title = ep
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let long_title = ep
                    .get("long_title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let full_title = if !ep_title.is_empty() && !long_title.is_empty() {
                    format!("第{}话 {}", ep_title, long_title)
                } else if !long_title.is_empty() {
                    long_title.clone()
                } else {
                    ep_title.clone()
                };

                let mut share_url = ep
                    .get("share_url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if share_url.is_empty() {
                    if let Some(ep_id) = ep.get("id").and_then(|v| v.as_i64()) {
                        share_url = format!(
                            "https://www.bilibili.com/bangumi/play/ep{}?season_id={}",
                            ep_id, season_id
                        );
                    }
                }
                if share_url.is_empty() {
                    continue;
                }

                let cover = ep
                    .get("cover")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                out.push(BangumiEpisode {
                    full_title,
                    number: if ep_title.is_empty() {
                        None
                    } else {
                        Some(ep_title)
                    },
                    cover,
                    share_url,
                });
            }
        }
    }

    // main_section.episodes
    if let Some(main_eps) = result.get("main_section").and_then(|m| m.get("episodes")) {
        extract_from_array(main_eps, season_id, &mut episodes);
    }

    // section[].episodes
    if let Some(sections) = result.get("section").and_then(|s| s.as_array()) {
        for sec in sections {
            if let Some(eps) = sec.get("episodes") {
                extract_from_array(eps, season_id, &mut episodes);
            }
        }
    }

    Ok(episodes)
}
