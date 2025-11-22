use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset};
use regex::Regex;
use scraper::{Html, Selector};
use serde::Deserialize;

const BASE_URL: &str = "https://www.scientificamerican.com";

#[derive(Debug, Deserialize)]
struct MetaTags {
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Meta {
    title: String,
    #[serde(rename = "canonicalUrl")]
    canonical_url: Option<String>,
    tags: Option<MetaTags>,
}

#[derive(Debug, Deserialize)]
struct RootProps {
    results: Vec<PodcastListItem>,
}

#[derive(Debug, Deserialize)]
struct RootInitialData {
    meta: Meta,
    props: RootProps,
}

#[derive(Debug, Deserialize)]
struct RootData {
    #[serde(rename = "initialData")]
    initial_data: RootInitialData,
}

#[derive(Debug, Deserialize)]
struct Author {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PodcastListItem {
    id: i64,
    title: String,
    summary: Option<String>,
    url: Option<String>,
    #[serde(rename = "image_url")]
    image_url: Option<String>,
    #[serde(rename = "image_alt_text")]
    image_alt_text: Option<String>,
    #[serde(rename = "media_url")]
    media_url: Option<String>,
    #[serde(rename = "media_type")]
    media_type: Option<String>,
    #[serde(rename = "release_date")]
    release_date: Option<String>,
    #[serde(rename = "date_published")]
    date_published: Option<String>,
    category: Option<String>,
    subtype: Option<String>,
    column: Option<String>,
    #[serde(rename = "digital_column")]
    digital_column: Option<String>,
    authors: Option<Vec<Author>>,
}

fn parse_date(s: &str) -> Option<DateTime<FixedOffset>> {
    util::parse_date(s)
}

fn extract_data_json(html: &str) -> Result<RootData> {
    let re = Regex::new(r#"window\.__DATA__=JSON\.parse\(`([\s\S]*?)`\)"#)
        .map_err(|e| Error::Parse(format!("scientificamerican: invalid data regex: {e}")))?;
    let caps = re
        .captures(html)
        .ok_or_else(|| Error::Parse("scientificamerican: __DATA__ JSON not found".to_string()))?;
    let inner = caps
        .get(1)
        .ok_or_else(|| Error::Parse("scientificamerican: empty __DATA__ capture".to_string()))?
        .as_str();
    // 同 RSSHub：先把 `\\` 替换为 `\` 再 JSON 解析。
    let json_str = inner.replace("\\\\", "\\");
    serde_json::from_str::<RootData>(&json_str)
        .map_err(|e| Error::Parse(format!("scientificamerican: invalid __DATA__ JSON: {e}")))
}

pub const META_SCIAM_PODCAST: RouteMeta = RouteMeta {
    hub_id: "scientificamerican/podcast",
    path: "/scientificamerican/podcast/:id?",
    categories: &["science", "podcast"],
    example: "/scientificamerican/podcast",
    params: &[
        ParamMeta {
            name: "id",
            description: "系列 ID，例如 science-quickly 或 science-talk，留空则为全部 Podcasts。",
            default: None,
            options: &[],
        },
        ParamMeta {
            name: "limit",
            description: "最大单集数量（默认 12）。",
            default: Some("12"),
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
            source: &[
                "www.scientificamerican.com/podcasts/",
                "www.scientificamerican.com/podcast/:id",
            ],
            target: "/podcast/:id?",
        },
        Radar {
            source: &["www.scientificamerican.com/podcast/science-quickly/"],
            target: "/podcast/science-quickly",
        },
        Radar {
            source: &["www.scientificamerican.com/podcast/science-talk/"],
            target: "/podcast/science-talk",
        },
    ],
    name: "Scientific American Podcasts",
    maintainers: &["captura"],
    url: "https://www.scientificamerican.com/podcasts/",
    description: "Scientific American 官方播客列表，可按节目子系列（如 Science Quickly）筛选。",
    default_view: Some("podcast"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let id_raw = ctx.param_str("id").unwrap_or("").trim().to_string();
    let limit = ctx.param_i64("limit").unwrap_or(12).max(1) as usize;

    let target_url = if id_raw.is_empty() {
        format!("{BASE_URL}/podcasts/")
    } else {
        format!("{BASE_URL}/podcast/{}/", id_raw)
    };

    let html = util::get_html(&target_url).await?;
    let doc = Html::parse_document(&html);

    // 基本元信息：标题、描述、语言、封面
    let sel_html = Selector::parse("html")
        .map_err(|e| Error::Parse(format!("scientificamerican: invalid html selector: {e}")))?;
    let sel_title = Selector::parse("title")
        .map_err(|e| Error::Parse(format!("scientificamerican: invalid title selector: {e}")))?;
    let sel_meta_desc = Selector::parse("meta[name=\"description\"]").map_err(|e| {
        Error::Parse(format!(
            "scientificamerican: invalid meta description selector: {e}"
        ))
    })?;
    let sel_og_image = Selector::parse("meta[property=\"og:image\"]").map_err(|e| {
        Error::Parse(format!(
            "scientificamerican: invalid og:image selector: {e}"
        ))
    })?;

    let lang = doc
        .select(&sel_html)
        .next()
        .and_then(|el| el.value().attr("lang"))
        .unwrap_or("en")
        .to_string();
    let page_title = doc
        .select(&sel_title)
        .next()
        .map(|t| t.text().collect::<String>().trim().to_string())
        .unwrap_or_else(|| "Scientific American Podcasts".to_string());
    let page_desc = doc
        .select(&sel_meta_desc)
        .next()
        .and_then(|m| m.value().attr("content"))
        .map(|s| s.to_string());
    let og_image = doc
        .select(&sel_og_image)
        .next()
        .and_then(|m| m.value().attr("content"))
        .map(|s| s.to_string());

    // 解析 window.__DATA__ JSON
    let root = match extract_data_json(&html) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(
                "scientificamerican/podcast: failed to parse __DATA__: {}",
                e
            );
            return Ok(HubData {
                title: page_title,
                description: Some(format!("Scientific American Podcasts 数据解析失败：{}", e)),
                link: Some(target_url),
                image: og_image,
                language: Some(lang),
                items: Vec::new(),
                allow_empty: true,
            });
        }
    };

    let meta = &root.initial_data.meta;
    let results = &root.initial_data.props.results;

    let feed_title = if !meta.title.trim().is_empty() {
        meta.title.trim().to_string()
    } else {
        page_title
    };

    let mut items = Vec::new();

    for item in results.iter().take(limit) {
        let title = item.title.trim().to_string();
        if title.is_empty() {
            continue;
        }

        let link = item.url.as_ref().map(|u| util::absolutize(BASE_URL, u));

        // 构造描述：audio + image + summary
        let mut desc_parts = Vec::new();

        if let Some(media_url) = item.media_url.as_ref() {
            if !media_url.is_empty() && item.media_type.as_deref() == Some("podcast") {
                desc_parts.push(format!(
                    "<p><audio controls src=\"{src}\">Your browser does not support the audio element.</audio></p>",
                    src = media_url
                ));
            }
        }

        if let Some(img) = item.image_url.as_ref() {
            if !img.is_empty() {
                let alt = item
                    .image_alt_text
                    .as_deref()
                    .unwrap_or(&title)
                    .replace('"', "");
                desc_parts.push(format!(
                    "<p><img src=\"{src}\" alt=\"{alt}\"></p>",
                    src = img,
                    alt = alt
                ));
            }
        }

        if let Some(summary) = item.summary.as_ref() {
            let s = summary.trim();
            if !s.is_empty() {
                desc_parts.push(format!("<p>{}</p>", s));
            }
        }

        let description = if desc_parts.is_empty() {
            None
        } else {
            Some(desc_parts.join("\n"))
        };

        let pub_date_str = item
            .release_date
            .as_ref()
            .or(item.date_published.as_ref())
            .map(|s| s.as_str())
            .unwrap_or("");
        let pub_date = if pub_date_str.is_empty() {
            None
        } else {
            parse_date(pub_date_str)
        };

        let author = item
            .authors
            .as_ref()
            .map(|authors| {
                authors
                    .iter()
                    .filter_map(|a| a.name.as_ref().map(|s| s.trim().to_string()))
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .filter(|s| !s.is_empty());

        let mut categories = Vec::new();
        for c in [
            item.category.as_ref(),
            item.subtype.as_ref(),
            item.column.as_ref(),
            item.digital_column.as_ref(),
        ] {
            if let Some(s) = c {
                let t = s.trim();
                if !t.is_empty() && !categories.contains(&t.to_string()) {
                    categories.push(t.to_string());
                }
            }
        }
        if !categories.iter().any(|c| c.eq_ignore_ascii_case("science")) {
            categories.push("Science".to_string());
        }
        if !categories.iter().any(|c| c.eq_ignore_ascii_case("podcast")) {
            categories.push("podcast".to_string());
        }

        items.push(HubItem {
            title,
            description,
            link,
            author,
            pub_date,
            categories,
        });
    }

    let feed_desc = meta
        .tags
        .as_ref()
        .and_then(|t| t.description.clone())
        .or(page_desc);

    Ok(HubData {
        title: feed_title,
        description: feed_desc,
        link: Some(target_url),
        image: og_image,
        language: Some(lang),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_SCIAM_PODCAST: Route = Route {
    meta: &META_SCIAM_PODCAST,
    handler: handler_fn,
};
