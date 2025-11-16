use captura_common::{Error, NormalizedEntry, Result};
use chrono::{TimeZone, Utc};
use serde_json::Value as JsonValue;

use crate::routes::bilibili::rules::utils::{
    bilibili_get_json, normalize_cover_url, render_ugc_description,
};

/// Options for user dynamic route, simplified from RSSHub's routeParams.
#[derive(Debug, Clone)]
pub struct DynamicOptions {
    pub show_emoji: bool,
    pub embed: bool,
    pub hide_goods: bool,
    pub direct_link: bool,
    pub use_avid: bool,
    pub offset: Option<String>,
}

impl Default for DynamicOptions {
    fn default() -> Self {
        Self {
            show_emoji: false,
            embed: true,
            hide_goods: false,
            direct_link: false,
            use_avid: false,
            offset: None,
        }
    }
}

/// Fetch user dynamic feed and map to normalized entries (simplified).
///
/// This is a lightweight adaptation of RSSHub's `/bilibili/user/dynamic`
/// route, focusing on the most common dynamic types. Anti-crawler handling
/// is limited: callers should provide `BILIBILI_COOKIE` in the environment
/// for best results.
pub async fn fetch_user_dynamic(uid: &str, opts: &DynamicOptions) -> Result<Vec<NormalizedEntry>> {
    if uid.trim().is_empty() {
        return Err(Error::Config("uid is required for user dynamic".into()));
    }

    let mut params = vec![
        format!("host_mid={}", uid),
        "platform=web".to_string(),
        "features=itemOpusStyle,listOnlyfans,opusBigCover,onlyfansVote".to_string(),
    ];
    if let Some(off) = &opts.offset {
        if !off.is_empty() {
            params.insert(0, format!("offset={}", off));
        }
    }
    let query = params.join("&");
    let url = format!(
        "https://api.bilibili.com/x/polymer/web-dynamic/v1/feed/space?{}",
        query
    );

    let referer = format!("https://space.bilibili.com/{}/", uid);
    let v = bilibili_get_json(&url, Some(&referer)).await?;

    let data = v
        .get("data")
        .ok_or_else(|| Error::Parse("dynamic: missing data".into()))?;
    let items = data
        .get("items")
        .and_then(|i| i.as_array())
        .ok_or_else(|| Error::Parse("dynamic: data.items is not an array".into()))?;

    let mut entries = Vec::new();

    for item in items {
        // Filter out goods if requested.
        if opts.hide_goods {
            if let Some(t) = item
                .get("modules")
                .and_then(|m| m.get("module_dynamic"))
                .and_then(|d| d.get("additional"))
                .and_then(|a| a.get("type"))
                .and_then(|t| t.as_str())
            {
                if t == "ADDITIONAL_TYPE_GOODS" {
                    continue;
                }
            }
        }

        let modules = match item.get("modules") {
            Some(m) => m,
            None => continue,
        };

        // Title: simplified getTitle implementation.
        let major = modules
            .get("module_dynamic")
            .and_then(|d| d.get("major"))
            .unwrap_or(&JsonValue::Null);

        let title_str = get_dynamic_title(major);

        // Base description text.
        let mut description = get_dynamic_description(
            modules
                .get("module_dynamic")
                .and_then(|d| d.get("desc"))
                .unwrap_or(&JsonValue::Null),
        );

        // Emoji / topic handling (simplified).
        let mut categories: Vec<String> = Vec::new();
        if let Some(nodes) = modules
            .get("module_dynamic")
            .and_then(|d| d.get("desc"))
            .and_then(|d| d.get("rich_text_nodes"))
            .and_then(|n| n.as_array())
        {
            for node in nodes {
                if let Some(t) = node.get("type").and_then(|t| t.as_str()) {
                    if t == "RICH_TEXT_NODE_TYPE_TOPIC" {
                        if let Some(text) = node.get("text").and_then(|t| t.as_str()) {
                            if let Some(name) = text.trim_matches('#').split('#').next() {
                                if !name.is_empty() {
                                    categories.push(name.to_string());
                                }
                            }
                        }
                    }
                }

                if opts.show_emoji {
                    // Replace emoji text with <img> icons.
                    if let Some(emoji) = node.get("emoji") {
                        if let (Some(text), Some(icon_url)) = (
                            emoji.get("text").and_then(|t| t.as_str()),
                            emoji.get("icon_url").and_then(|u| u.as_str()),
                        ) {
                            if !text.is_empty() && !icon_url.is_empty() {
                                let img = format!(
                                    "<img alt=\"{}\" src=\"{}\" style=\"margin: -1px 1px 0px; display: inline-block; width: 20px; height: 20px; vertical-align: text-bottom;\" referrerpolicy=\"no-referrer\">",
                                    text, icon_url
                                );
                                description = description.replace(text, &img);
                            }
                        }
                    }

                    // Replace pics-in-text nodes with <img>.
                    if let Some(pics) = node.get("pics").and_then(|p| p.as_array()) {
                        if let Some(text) = node.get("text").and_then(|t| t.as_str()) {
                            let html = pics
                                .iter()
                                .filter_map(|pic| {
                                    let url = pic.get("src").and_then(|u| u.as_str())?;
                                    let w = pic.get("width").and_then(|w| w.as_i64());
                                    let h = pic.get("height").and_then(|h| h.as_i64());
                                    Some(format!(
                                        "<img alt=\"{text}\" src=\"{url}\" style=\"margin: 0; display: inline-block;{}{} vertical-align: text-bottom;\" referrerpolicy=\"no-referrer\">",
                                        w.map(|w| format!(" width:{w}px;")).unwrap_or_default(),
                                        h.map(|h| format!(" height:{h}px;")).unwrap_or_default(),
                                    ))
                                })
                                .collect::<Vec<_>>()
                                .join("<br>");
                            if !html.is_empty() {
                                description = description.replace(text, &html);
                            }
                        }
                    }
                }
            }
        }

        // Cover and bvid/aid for embed.
        let (cover, bvid, aid) = extract_cover_bvid_aid(major);

        // Link: fallback to dynamic page, optionally direct link.
        let default_link = item
            .get("id_str")
            .and_then(|id| id.as_str())
            .map(|id| format!("https://t.bilibili.com/{}", id))
            .unwrap_or_else(|| format!("https://space.bilibili.com/{}/dynamic", uid));

        let direct_link = if opts.direct_link {
            extract_direct_link(major, opts.use_avid, bvid.as_deref(), aid)
        } else {
            None
        };

        let link = direct_link.unwrap_or(default_link);

        // Append origin description if present (simplified).
        if let Some(origin_modules) = item.get("orig").and_then(|o| o.get("modules")) {
            let origin = build_origin_description(origin_modules);
            if !origin.is_empty() {
                if !description.is_empty() {
                    description.push_str("<br>");
                }
                description.push_str(&origin);
            }
        }

        // Merge summary and embed into HTML description.
        if !description.is_empty() {
            description = description.replace("\r\n", "<br>").replace('\n', "<br>");
        }

        let html = render_ugc_description(
            opts.embed,
            cover.as_deref(),
            &description,
            bvid.as_deref(),
            aid,
        );

        // Author and pub_ts.
        let author = modules
            .get("module_author")
            .and_then(|a| a.get("name"))
            .and_then(|n| n.as_str())
            .map(|s| s.to_string());

        let pub_ts = modules
            .get("module_author")
            .and_then(|a| a.get("pub_ts"))
            .and_then(|t| t.as_i64());

        let published_at = pub_ts.and_then(|ts| Utc.timestamp_opt(ts, 0).single());

        entries.push(NormalizedEntry {
            guid: Some(link.clone()),
            url: Some(link),
            title: Some(title_str.clone().unwrap_or_else(|| description.clone())),
            summary: Some(description.clone()),
            content_html: Some(html),
            author,
            published_at,
            enclosures: Vec::new(),
            extras: serde_json::json!({ "categories": categories }),
        });
    }

    Ok(entries)
}

fn get_dynamic_title(major: &JsonValue) -> Option<String> {
    if major.is_null() {
        return None;
    }
    if let Some(tips) = major
        .get("none")
        .and_then(|n| n.get("tips"))
        .and_then(|t| t.as_str())
    {
        return Some(tips.to_string());
    }
    if let Some(courses) = major.get("courses") {
        let title = courses
            .get("title")
            .and_then(|t| t.as_str())
            .unwrap_or_default();
        let sub = courses
            .get("sub_title")
            .and_then(|t| t.as_str())
            .unwrap_or_default();
        if !title.is_empty() {
            return Some(format!("{} - {}", title, sub));
        }
    }
    if let Some(content) = major
        .get("live_rcmd")
        .and_then(|r| r.get("content"))
        .and_then(|c| c.as_str())
    {
        if let Ok(v) = serde_json::from_str::<JsonValue>(content) {
            if let Some(t) = v
                .get("live_play_info")
                .and_then(|p| p.get("title"))
                .and_then(|t| t.as_str())
            {
                return Some(t.to_string());
            }
        }
    }
    if let Some(t) = major.get("type").and_then(|t| t.as_str()) {
        let key = t.strip_prefix("MAJOR_TYPE_").unwrap_or(t).to_lowercase();
        if let Some(obj) = major.get(&key) {
            if let Some(title) = obj.get("title").and_then(|t| t.as_str()) {
                if !title.is_empty() {
                    return Some(title.to_string());
                }
            }
        }
    }
    None
}

fn get_dynamic_description(desc_node: &JsonValue) -> String {
    desc_node
        .get("text")
        .and_then(|t| t.as_str())
        .unwrap_or_default()
        .to_string()
}

fn build_origin_description(origin_modules: &JsonValue) -> String {
    let mut buf = String::new();

    // Origin author name.
    if let Some(name) = origin_modules
        .get("module_author")
        .and_then(|a| a.get("name"))
        .and_then(|n| n.as_str())
    {
        buf.push_str(&format!("//转发自: @{}: ", name));
    }

    let origin_major = origin_modules
        .get("module_dynamic")
        .and_then(|d| d.get("major"))
        .unwrap_or(&JsonValue::Null);
    let origin_desc_node = origin_modules
        .get("module_dynamic")
        .and_then(|d| d.get("desc"))
        .unwrap_or(&JsonValue::Null);

    if let Some(title) = get_dynamic_title(origin_major) {
        buf.push_str(&title);
    }

    let origin_text = get_dynamic_description(origin_desc_node);
    if !origin_text.is_empty() {
        if !buf.is_empty() {
            buf.push_str("<br>");
        }
        buf.push_str(&origin_text);
    }

    buf
}

fn extract_cover_bvid_aid(major: &JsonValue) -> (Option<String>, Option<String>, Option<i64>) {
    if major.is_null() {
        return (None, None, None);
    }

    // Prefer archive cover / bvid when present.
    if let Some(archive) = major.get("archive") {
        let cover = archive
            .get("cover")
            .and_then(|c| c.as_str())
            .map(|s| normalize_cover_url(s));
        let bvid = archive
            .get("bvid")
            .and_then(|b| b.as_str())
            .map(|s| s.to_string());
        let aid = archive.get("aid").and_then(|a| a.as_i64());
        return (cover, bvid, aid);
    }

    // Opus cover.
    if let Some(opus) = major.get("opus") {
        if let Some(pics) = opus.get("pics").and_then(|p| p.as_array()) {
            if let Some(first) = pics.first() {
                let cover = first
                    .get("url")
                    .and_then(|u| u.as_str())
                    .map(|s| normalize_cover_url(s));
                return (cover, None, None);
            }
        }
    }

    // Live cover.
    if let Some(live) = major.get("live") {
        let cover = live
            .get("cover")
            .and_then(|c| c.as_str())
            .map(|s| normalize_cover_url(s));
        return (cover, None, None);
    }

    (None, None, None)
}

fn extract_direct_link(
    major: &JsonValue,
    use_avid: bool,
    _bvid: Option<&str>,
    _aid: Option<i64>,
) -> Option<String> {
    if major.is_null() {
        return None;
    }

    // Video archive → video page.
    if let Some(archive) = major.get("archive") {
        if use_avid {
            if let Some(a) = archive.get("aid").and_then(|a| a.as_i64()) {
                return Some(format!("https://www.bilibili.com/video/av{}", a));
            }
        }
        if let Some(b) = archive.get("bvid").and_then(|b| b.as_str()) {
            if !b.is_empty() {
                return Some(format!("https://www.bilibili.com/video/{}", b));
            }
        }
        if !use_avid {
            if let Some(a) = archive.get("aid").and_then(|a| a.as_i64()) {
                return Some(format!("https://www.bilibili.com/video/av{}", a));
            }
        }
    }

    // Article → read page.
    if let Some(article) = major.get("article") {
        if let Some(id) = article.get("id").and_then(|i| i.as_i64()) {
            return Some(format!("https://www.bilibili.com/read/cv{}", id));
        }
    }

    // Opus → jump_url (usually article or draw).
    if let Some(opus) = major.get("opus") {
        if let Some(jump) = opus.get("jump_url").and_then(|j| j.as_str()) {
            if jump.starts_with("http") {
                return Some(jump.to_string());
            } else if let Some(rest) = jump.strip_prefix("//") {
                return Some(format!("https://{}", rest));
            }
        }
    }

    // Live room share.
    if let Some(live) = major.get("live") {
        if let Some(id) = live.get("id").and_then(|i| i.as_i64()) {
            return Some(format!("https://live.bilibili.com/{}", id));
        }
    }

    // Live recommendation.
    if let Some(content) = major
        .get("live_rcmd")
        .and_then(|r| r.get("content"))
        .and_then(|c| c.as_str())
    {
        if let Ok(v) = serde_json::from_str::<JsonValue>(content) {
            if let Some(room_id) = v
                .get("live_play_info")
                .and_then(|p| p.get("room_id"))
                .and_then(|r| r.as_i64())
            {
                return Some(format!("https://live.bilibili.com/{}", room_id));
            }
        }
    }

    None
}
