use crate::routes::types::HubItem;
use crate::routes::util;
use captura_common::Result;
use chrono::{DateTime, FixedOffset};
use serde::Deserialize;
use std::collections::HashMap;

pub const BASE_URL: &str = "https://www.gcores.com";
pub const IMAGE_BASE_URL: &str = "https://image.gcores.com";
pub const AUDIO_BASE_URL: &str = "https://alioss.gcores.com";

#[derive(Debug, Deserialize, Clone)]
pub struct ApiItem {
    pub id: String,
    #[serde(default)]
    pub r#type: String,
    #[serde(default)]
    pub attributes: Attributes,
    #[serde(default)]
    pub relationships: Relationships,
}

#[derive(Debug, Default, Deserialize, Clone)]
pub struct Attributes {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub desc: Option<String>,
    #[serde(default)]
    pub excerpt: Option<String>,
    #[serde(default)]
    pub cover: Option<String>,
    #[serde(default)]
    pub thumb: Option<String>,
    #[serde(default, rename = "published-at")]
    pub published_at: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default, rename = "speech-path")]
    pub speech_path: Option<String>,
    #[serde(default)]
    pub duration: Option<i64>,
}

#[derive(Debug, Default, Deserialize, Clone)]
pub struct Relationships {
    #[serde(default)]
    pub category: Option<RelData>,
    #[serde(default)]
    pub tag: Option<RelData>,
    #[serde(default)]
    pub topic: Option<RelData>,
    #[serde(default)]
    pub user: Option<RelData>,
    #[serde(default)]
    pub media: Option<RelData>,
}

#[derive(Debug, Default, Deserialize, Clone)]
pub struct RelData {
    pub data: Option<RelRef>,
}

#[derive(Debug, Default, Deserialize, Clone)]
pub struct RelRef {
    pub id: String,
    #[serde(default)]
    pub r#type: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum DataOrList {
    One(ApiItem),
    Many(Vec<ApiItem>),
}

#[derive(Debug, Deserialize)]
pub struct ApiResponse {
    pub data: DataOrList,
    #[serde(default)]
    pub included: Vec<ApiItem>,
}

/// Draft.js 风格内容结构（精简版，只保留我们用到的字段）。
#[derive(Debug, Deserialize)]
struct DraftContent {
    #[serde(default)]
    blocks: Vec<DraftBlock>,
    #[serde(default, rename = "entityMap")]
    entity_map: HashMap<String, DraftEntity>,
}

#[derive(Debug, Deserialize)]
struct DraftBlock {
    #[serde(default)]
    text: String,
    #[serde(default, rename = "type")]
    kind: String,
}

#[derive(Debug, Deserialize)]
struct DraftEntity {
    #[serde(default, rename = "type")]
    kind: String,
    #[serde(default)]
    data: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct MetaOgUrl {
    #[serde(default)]
    pub content: Option<String>,
}

/// 将 API items + included 转换为 HubItem 列表。
pub async fn process_items(
    limit: usize,
    query: Option<&serde_json::Map<String, serde_json::Value>>,
    api_url: &str,
    target_url: &str,
    default_view: Option<&'static str>,
) -> Result<(
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Vec<HubItem>,
)> {
    // 请求 API JSON
    let mut api_url_full = api_url.to_string();
    if let Some(q) = query {
        let qs = serde_urlencoded::to_string(q).unwrap_or_default();
        if !qs.is_empty() {
            api_url_full.push('?');
            api_url_full.push_str(&qs);
        }
    }
    let resp: ApiResponse = util::get_json(&api_url_full).await?;

    // 请求目标 HTML 以获取语言和标题等信息。
    let html = util::get_html(target_url).await?;
    let doc = scraper::Html::parse_document(&html);
    let sel_title = scraper::Selector::parse("title").unwrap();
    let sel_lang = scraper::Selector::parse("html").unwrap();
    let sel_meta_desc = scraper::Selector::parse(r#"meta[name="description"]"#).unwrap();
    let sel_meta_og = scraper::Selector::parse(r#"meta[property="og:url"]"#).unwrap();

    let title = doc
        .select(&sel_title)
        .next()
        .map(|t| util::element_text(&t))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "GCORES".to_string());
    let language = doc
        .select(&sel_lang)
        .next()
        .and_then(|h| h.value().attr("lang"))
        .map(|s| s.to_string())
        .or_else(|| Some("zh-CN".to_string()));
    let description = doc
        .select(&sel_meta_desc)
        .next()
        .and_then(|m| m.value().attr("content"))
        .map(|s| s.to_string());
    let og_url = doc
        .select(&sel_meta_og)
        .next()
        .and_then(|m| m.value().attr("content"))
        .map(|s| s.to_string());

    // 构建 included 查找表。
    let mut included_map: HashMap<(String, String), ApiItem> = HashMap::new();
    for item in resp.included.iter() {
        included_map.insert((item.r#type.clone(), item.id.clone()), item.clone());
    }

    let mut items_out = Vec::new();
    let mut all: Vec<ApiItem> = match resp.data {
        DataOrList::One(it) => vec![it],
        DataOrList::Many(list) => list,
    };
    all.extend(resp.included.clone());

    for item in all.into_iter().filter(|i| {
        matches!(
            i.r#type.as_str(),
            "radios" | "articles" | "news" | "videos" | "talks"
        )
    }) {
        if items_out.len() >= limit {
            break;
        }

        let attrs = &item.attributes;
        let rel = &item.relationships;

        let mut categories = Vec::new();
        for r in [&rel.category, &rel.tag, &rel.topic] {
            if let Some(RelData { data: Some(ref d) }) = r {
                if let Some(found) = included_map.get(&(d.r#type.clone(), d.id.clone())) {
                    if let Some(name) = found.attributes.title.clone() {
                        if !name.is_empty() {
                            categories.push(name);
                        }
                    }
                }
            }
        }

        let pub_date: Option<DateTime<FixedOffset>> =
            attrs.published_at.as_deref().and_then(util::parse_date);

        let link = format!("{}/{}/{}", BASE_URL, item.r#type, item.id);

        let image = attrs
            .cover
            .as_ref()
            .or(attrs.thumb.as_ref())
            .map(|p| format!("{}/{}", IMAGE_BASE_URL, p.trim_start_matches('/')));

        let mut description_html = String::new();

        if let Some(ref img) = image {
            description_html.push_str("<p>");
            let alt = attrs.title.clone().unwrap_or_else(|| "GCORES".to_string());
            description_html.push_str(&util::html_img(img, &alt));
            description_html.push_str("</p>");
        }

        if let Some(ref intro) = attrs.desc {
            if !intro.is_empty() {
                description_html.push_str("<p>");
                description_html.push_str(intro);
                description_html.push_str("</p>");
            }
        } else if let Some(ref excerpt) = attrs.excerpt {
            if !excerpt.is_empty() {
                description_html.push_str("<p>");
                description_html.push_str(excerpt);
                description_html.push_str("</p>");
            }
        }

        // Draft.js 正文（主要用于 talks，文章等也可兼容）。
        if let Some(ref content_json) = attrs.content {
            if let Some(html) = parse_draft_content(content_json) {
                if !html.is_empty() {
                    description_html.push_str(&html);
                }
            }
        }

        let title = attrs
            .title
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(|| attrs.desc.clone())
            .unwrap_or_else(|| "GCORES".to_string());

        items_out.push(HubItem {
            title,
            description: if description_html.is_empty() {
                None
            } else {
                Some(description_html)
            },
            link: Some(link),
            author: None,
            pub_date,
            categories,
        });
    }

    Ok((
        title,
        description,
        og_url.or_else(|| Some(target_url.to_string())),
        language,
        items_out,
    ))
}

/// 解析 GCORES 使用的 Draft.js JSON 内容为简单 HTML。
fn parse_draft_content(json_str: &str) -> Option<String> {
    let content: DraftContent = serde_json::from_str(json_str).ok()?;

    let mut html = String::new();

    // 先渲染图片类实体（IMAGE / GALLERY），避免复杂的 inline entity 拼装。
    for entity in content.entity_map.values() {
        match entity.kind.as_str() {
            "IMAGE" => {
                if let Some(path) = entity.data.get("path").and_then(|v| v.as_str()) {
                    let src = format!("{}/{}", IMAGE_BASE_URL, path.trim_start_matches('/'));
                    let alt = entity
                        .data
                        .get("caption")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    html.push_str("<figure>");
                    html.push_str(&util::html_img(&src, alt));
                    html.push_str("</figure>");
                }
            }
            "GALLERY" => {
                if let Some(images) = entity.data.get("images").and_then(|v| v.as_array()) {
                    for img in images {
                        if let Some(path) = img.get("path").and_then(|v| v.as_str()) {
                            let src =
                                format!("{}/{}", IMAGE_BASE_URL, path.trim_start_matches('/'));
                            let alt = img
                                .get("caption")
                                .and_then(|v| v.as_str())
                                .unwrap_or_default();
                            html.push_str("<figure>");
                            html.push_str(&util::html_img(&src, alt));
                            html.push_str("</figure>");
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // 再渲染文本块。
    for block in content.blocks.into_iter() {
        let text = block.text.trim();
        if text.is_empty() {
            continue;
        }
        let tag = match block.kind.as_str() {
            "header-one" => "h1",
            "header-two" => "h2",
            "header-three" => "h3",
            "header-four" => "h4",
            "header-five" => "h5",
            "header-six" => "h6",
            "blockquote" => "blockquote",
            "unordered-list-item" | "ordered-list-item" => "p",
            "code-block" => "pre",
            _ => "p",
        };
        html.push_str("<");
        html.push_str(tag);
        html.push('>');
        html.push_str(text);
        html.push_str("</");
        html.push_str(tag);
        html.push('>');
    }

    if html.is_empty() { None } else { Some(html) }
}
