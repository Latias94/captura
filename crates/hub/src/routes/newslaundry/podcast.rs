use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;
use chrono::{DateTime, FixedOffset};
use serde_json::Value;

const ROOT_URL: &str = "https://www.newslaundry.com";

fn parse_ms_ts(ts: i64) -> Option<DateTime<FixedOffset>> {
    // Newslaundry 时间戳为毫秒级 Unix 时间，近似按 UTC 处理。
    crate::routes::util::parse_ms_timestamp(ts, 0)
}

pub const META_NEWSLAUNDRY_PODCAST: RouteMeta = RouteMeta {
    hub_id: "newslaundry/podcast",
    path: "/newslaundry/podcast/:category?",
    categories: &["new-media"],
    example: "/newslaundry/podcast",
    params: &[
        ParamMeta {
            name: "category",
            description: "播客分类，可选：nl-hafta、whats-your-ism，留空为全部。",
            default: None,
            options: &[
                ("nl-hafta", "NL Hafta"),
                ("whats-your-ism", "What's Your Ism?"),
            ],
        },
        ParamMeta {
            name: "limit",
            description: "最大条目数量（默认 20）。",
            default: Some("20"),
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
    radar: &[
        Radar {
            source: &["newslaundry.com/podcast"],
            target: "/podcast",
        },
        Radar {
            source: &["newslaundry.com/collection/nl-hafta-podcast"],
            target: "/podcast/nl-hafta",
        },
        Radar {
            source: &["newslaundry.com/podcast/whats-your-ism"],
            target: "/podcast/whats-your-ism",
        },
    ],
    name: "Newslaundry Podcast",
    maintainers: &["captura"],
    url: "https://www.newslaundry.com/podcast",
    description:
        "Newslaundry 播客页，对标 RSSHub /newslaundry/podcast/:category，基于官方 collections API 构造带图文与内嵌播放器的条目。",
    default_view: Some("podcast"),
};

async fn fetch_collection(
    slug: &str,
    custom_url: Option<String>,
    skip_first: bool,
    limit: usize,
) -> Result<HubData> {
    let api_url = format!("{}/api/v1/collections/{}", ROOT_URL, slug);
    let current_url = custom_url.unwrap_or_else(|| format!("{}/{}", ROOT_URL, slug));

    let client = client_basic(None, None).map_err(|e| Error::Network(e.to_string()))?;
    let resp = client
        .get(&api_url)
        .send()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "newslaundry collection: {} -> http status {}",
            api_url, status
        )));
    }
    let value: Value = resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("newslaundry collection json: {e}")))?;

    let name = value
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("Podcast")
        .trim()
        .to_string();
    let summary = value
        .get("summary")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();

    let items_raw = value
        .get("items")
        .and_then(|v| v.as_array())
        .ok_or_else(|| Error::Parse("newslaundry: items array missing".to_string()))?;
    if items_raw.is_empty() {
        return Err(Error::Parse("newslaundry: no items".to_string()));
    }

    let slice = if skip_first && items_raw.len() > 1 {
        &items_raw[1..]
    } else {
        &items_raw[..]
    };

    let mut items = Vec::new();

    for item in slice.iter().take(limit) {
        let story = match item.get("story") {
            Some(s) => s,
            None => continue,
        };

        let title = story
            .get("headline")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if title.is_empty() {
            continue;
        }

        let link = story
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if link.is_empty() {
            continue;
        }

        let hero_image_key = story
            .get("hero-image-s3-key")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let hero_image = if hero_image_key.is_empty() {
            None
        } else {
            Some(format!(
                "https://media.assettype.com/{}?auto=format%2Ccompress&fit=max&dpr=1.0&format=webp",
                hero_image_key
            ))
        };
        let hero_alt = story
            .get("hero-image-alt-text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        let hero_caption = story
            .get("hero-image-caption")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();

        let mut desc = String::new();
        if let Some(img) = hero_image.as_ref() {
            desc.push_str(&format!(
                r#"<p><img src="{src}" alt="{alt}"></p>"#,
                src = img,
                alt = hero_alt
            ));
            if !hero_caption.is_empty() {
                desc.push_str(&format!("<p><em>{}</em></p>", hero_caption));
            }
        }

        // cards / story-elements
        if let Some(cards) = story.get("cards").and_then(|v| v.as_array()) {
            for card in cards {
                if let Some(elements) = card.get("story-elements").and_then(|v| v.as_array()) {
                    for el in elements {
                        let kind = el.get("type").and_then(|v| v.as_str()).unwrap_or("").trim();
                        match kind {
                            "text" => {
                                if let Some(html) = el.get("text").and_then(|v| v.as_str()) {
                                    if !html.trim().is_empty() {
                                        desc.push_str(html);
                                    }
                                }
                            }
                            "image" => {
                                if let Some(img_key) =
                                    el.get("image-s3-key").and_then(|v| v.as_str())
                                {
                                    let url = format!(
                                        "https://media.assettype.com/{}?auto=format%2Ccompress&format=webp",
                                        img_key
                                    );
                                    let alt =
                                        el.get("alt-text").and_then(|v| v.as_str()).unwrap_or("");
                                    let title_attr =
                                        el.get("title").and_then(|v| v.as_str()).unwrap_or("");
                                    desc.push_str(&format!(
                                        r#"<p><img src="{src}" alt="{alt}" title="{title}"></p>"#,
                                        src = url,
                                        alt = alt,
                                        title = title_attr
                                    ));
                                }
                            }
                            "jsembed" => {
                                if let Some(embed_js) = el.get("embed-js").and_then(|v| v.as_str())
                                {
                                    if let Ok(bytes) = base64::decode(embed_js) {
                                        if let Ok(html) = String::from_utf8(bytes) {
                                            if !html.trim().is_empty() {
                                                desc.push_str(&html);
                                            }
                                        }
                                    }
                                }
                            }
                            "youtube-video" => {
                                if let Some(url) = el.get("url").and_then(|v| v.as_str()) {
                                    desc.push_str(&format!(
                                        r#"<p><a href="{href}">YouTube: {href}</a></p>"#,
                                        href = url
                                    ));
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        if desc.is_empty() {
            // 兜底使用副标题作为简要内容
            if let Some(sub) = story
                .get("subheadline")
                .and_then(|v| v.as_str())
                .filter(|s| !s.trim().is_empty())
            {
                desc.push_str(&format!("<p>{}</p>", sub.trim()));
            }
        }

        let pub_date = story
            .get("published-at")
            .and_then(|v| v.as_i64())
            .and_then(parse_ms_ts);

        let authors = story
            .get("authors")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|a| a.get("name").and_then(|v| v.as_str()))
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let author = if authors.is_empty() {
            story
                .get("author-name")
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
        } else {
            Some(authors.join(", "))
        };

        let categories = story
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| t.get("name").and_then(|v| v.as_str()))
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        items.push(HubItem {
            title,
            description: if desc.is_empty() { None } else { Some(desc) },
            link: Some(link),
            author,
            pub_date,
            categories,
        });
    }

    Ok(HubData {
        title: format!("{} - Newslaundry", name.trim()),
        description: if summary.is_empty() {
            Some(format!("{} articles from Newslaundry", name.trim()))
        } else {
            Some(summary)
        },
        link: Some(current_url),
        image: Some(format!("{}/favicon.ico", ROOT_URL)),
        language: Some("en".to_string()),
        items,
        allow_empty: false,
    })
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let category = ctx.param_str("category");
    let limit = ctx.param_i64("limit").unwrap_or(20).max(1) as usize;

    // 与 RSSHub 一致的分类映射
    let (slug, custom_url, skip_first) = match category {
        Some("nl-hafta") => (
            "nl-hafta-podcast".to_string(),
            Some(format!("{}/collection/nl-hafta-podcast", ROOT_URL)),
            false,
        ),
        Some("whats-your-ism") => (
            "whats-your-ism-podcast-newslaundry-hindi".to_string(),
            Some(format!("{}/podcast/whats-your-ism", ROOT_URL)),
            false,
        ),
        Some(_) => ("podcast".to_string(), None, true),
        None => ("podcast".to_string(), None, true),
    };

    let data = fetch_collection(&slug, custom_url, skip_first, limit).await?;
    Ok(data)
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_NEWSLAUNDRY_PODCAST: Route = Route {
    meta: &META_NEWSLAUNDRY_PODCAST,
    handler: handler_fn,
};
