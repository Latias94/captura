//! cnBeta.COM related routes.
//!
//! Currently implemented:
//! - /cnbeta                 Homepage stream (headlines / latest)
//! - /cnbeta/category/:id    Category stream
//! - /cnbeta/topics/:id      Topic stream

pub mod category;
pub mod index;
pub mod topics;

use crate::routes::types::HubItem;
use crate::routes::util;
use captura_common::{Error, Result};
use captura_net::client_basic;
use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeZone, Utc};
use scraper::{Html, Selector};
use serde::Deserialize;
use serde_json::Value;
use url::form_urlencoded;

/// Root URL of cnBeta.COM (Taiwan mirror used by RSSHub).
pub const ROOT_URL: &str = "https://www.cnbeta.com.tw";

#[derive(Debug, Deserialize)]
struct ApiResponse {
    state: String,
    #[serde(default)]
    message: String,
    result: Value,
    #[serde(default)]
    token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Label {
    name: String,
}

#[derive(Debug, Deserialize)]
struct ListEntry {
    title: String,
    #[serde(default)]
    hometext: String,
    #[serde(default)]
    source: String,
    #[serde(default)]
    inputtime: String,
    url_show: String,
    label: Label,
}

/// Logical kind of cnBeta feed we are fetching.
pub enum CnbetaKind {
    Index,
    Category { id: String },
    Topics { id: String },
}

fn encode_component(input: &str) -> String {
    let encoded = form_urlencoded::Serializer::new(String::new())
        .append_pair("k", input)
        .finish();
    encoded
        .split_once('=')
        .map(|(_, v)| v.to_string())
        .unwrap_or_default()
}

fn normalize_link(raw: &str) -> String {
    if raw.starts_with("//") {
        format!("https:{}", raw)
    } else {
        raw.replace("http:", "https:")
    }
}

fn extract_page_info(html: &str) -> Result<(String, String, String, Option<String>)> {
    let doc = Html::parse_document(html);

    let sel_token = Selector::parse(r#"meta[name="csrf-token"]"#)
        .map_err(|e| Error::Parse(format!("cnbeta: token selector error: {e}")))?;
    let sel_type = Selector::parse(r#"div[data-type]"#)
        .map_err(|e| Error::Parse(format!("cnbeta: type selector error: {e}")))?;
    let sel_title = Selector::parse("title")
        .map_err(|e| Error::Parse(format!("cnbeta: title selector error: {e}")))?;
    let sel_desc = Selector::parse(r#"meta[name="description"]"#)
        .map_err(|e| Error::Parse(format!("cnbeta: desc selector error: {e}")))?;

    let token = doc
        .select(&sel_token)
        .next()
        .and_then(|el| el.value().attr("content"))
        .ok_or_else(|| Error::Parse("cnbeta: csrf token not found".to_string()))?
        .to_string();

    let list_type = doc
        .select(&sel_type)
        .next()
        .and_then(|el| el.value().attr("data-type"))
        .ok_or_else(|| Error::Parse("cnbeta: data-type not found".to_string()))?
        .to_string();

    let title = doc
        .select(&sel_title)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .unwrap_or_else(|| "cnBeta.COM".to_string());

    let description = doc
        .select(&sel_desc)
        .next()
        .and_then(|el| el.value().attr("content"))
        .map(|s| s.to_string());

    Ok((token, list_type, title, description))
}

fn parse_inputtime_cn(s: &str) -> Option<DateTime<FixedOffset>> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return None;
    }
    let naive = NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M"))
        .ok()?;
    let offset = FixedOffset::east_opt(8 * 3600)?;
    offset.from_local_datetime(&naive).single()
}

async fn enrich_items(items: Vec<HubItem>, limit: usize) -> Result<Vec<HubItem>> {
    let mut out = Vec::new();

    for mut item in items.into_iter().take(limit) {
        let Some(link) = item.link.clone() else {
            out.push(item);
            continue;
        };

        let html = util::get_html(&link).await?;
        let doc = Html::parse_document(&html);

        let sel_summary = Selector::parse(".article-summary")
            .map_err(|e| Error::Parse(format!("cnbeta: summary selector error: {e}")))?;
        let sel_content = Selector::parse(".article-content")
            .map_err(|e| Error::Parse(format!("cnbeta: content selector error: {e}")))?;
        let sel_meta_source = Selector::parse("header.title div.meta span.source")
            .map_err(|e| Error::Parse(format!("cnbeta: source selector error: {e}")))?;
        let sel_meta_time = Selector::parse(".meta span")
            .map_err(|e| Error::Parse(format!("cnbeta: time selector error: {e}")))?;

        let summary_html = doc
            .select(&sel_summary)
            .next()
            .map(|el| el.html())
            .unwrap_or_default();
        let content_html = doc
            .select(&sel_content)
            .next()
            .map(|el| el.html())
            .unwrap_or_default();

        if !summary_html.is_empty() || !content_html.is_empty() {
            let mut desc = String::new();
            if !summary_html.is_empty() {
                desc.push_str(&summary_html);
            }
            if !content_html.is_empty() {
                desc.push_str(&content_html);
            }
            item.description = Some(desc);
        }

        if item.author.is_none() {
            if let Some(source_el) = doc.select(&sel_meta_source).next() {
                let text = source_el.text().collect::<String>().trim().to_string();
                if !text.is_empty() {
                    item.author = Some(text);
                }
            }
        }

        if item.pub_date.is_none() {
            if let Some(time_el) = doc.select(&sel_meta_time).next() {
                let text = time_el.text().collect::<String>().trim().to_string();
                if !text.is_empty() {
                    item.pub_date = util::parse_cn_datetime(&text).or_else(|| {
                        // Fallback: sometimes the text is a plain timestamp.
                        parse_inputtime_cn(&text)
                    });
                }
            }
        }

        out.push(item);
    }

    Ok(out)
}

/// Fetch cnBeta items for the given kind, following RSSHub's `/cnbeta` logic.
pub async fn fetch_cnbeta(
    kind: CnbetaKind,
    limit: usize,
) -> Result<(Vec<HubItem>, String, Option<String>)> {
    let current_url = match &kind {
        CnbetaKind::Index => ROOT_URL.to_string(),
        CnbetaKind::Category { id } => format!("{}/category/{}.htm", ROOT_URL, id),
        CnbetaKind::Topics { id } => format!("{}/topics/{}.htm", ROOT_URL, id),
    };

    let html = util::get_html(&current_url).await?;
    let (token_raw, list_type, title, description) = extract_page_info(&html)?;
    let token = encode_component(&token_raw);

    let ts = Utc::now().timestamp_millis();
    let api_url = format!(
        "{}/home/more?&type={}&page=1&_csrf={}&_={}",
        ROOT_URL, list_type, token, ts
    );

    let client = client_basic(None, None)
        .map_err(|e| Error::Network(format!("cnbeta client error: {}", e)))?;
    let resp = client
        .get(&api_url)
        .header("X-Requested-With", "XMLHttpRequest")
        .header("X-CSRF-TOKEN", token_raw)
        .header("Referer", &current_url)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{} -> {}", api_url, e)))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "{} -> http status {}",
            api_url, status
        )));
    }
    let api: ApiResponse = resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("cnbeta json parse error: {}", e)))?;

    // Root uses `result` directly, category/topics use `result.list`.
    let is_typed = matches!(
        kind,
        CnbetaKind::Category { .. } | CnbetaKind::Topics { .. }
    );
    let list_values = if is_typed {
        api.result
            .get("list")
            .and_then(|v| v.as_array())
            .ok_or_else(|| Error::Parse("cnbeta: result.list is not array".to_string()))?
            .clone()
    } else {
        api.result
            .as_array()
            .ok_or_else(|| Error::Parse("cnbeta: result is not array".to_string()))?
            .clone()
    };

    let mut items_pre = Vec::new();

    for v in list_values.into_iter().take(limit) {
        let entry: ListEntry = serde_json::from_value(v)
            .map_err(|e| Error::Parse(format!("cnbeta: invalid list item: {e}")))?;

        let link = Some(normalize_link(&entry.url_show));
        let categories = if entry.label.name.is_empty() {
            Vec::new()
        } else {
            vec![entry.label.name]
        };

        if is_typed {
            let author = if entry.source.is_empty() {
                None
            } else {
                Some(entry.source.split("@http").next().unwrap_or("").to_string())
            };
            let pub_date = if entry.inputtime.is_empty() {
                None
            } else {
                parse_inputtime_cn(&entry.inputtime)
            };
            let description = if entry.hometext.trim().is_empty() {
                None
            } else {
                Some(entry.hometext.clone())
            };

            items_pre.push(HubItem {
                title: entry.title,
                description,
                link,
                author,
                pub_date,
                categories,
            });
        } else {
            items_pre.push(HubItem {
                title: entry.title,
                description: None,
                link,
                author: None,
                pub_date: None,
                categories,
            });
        }
    }

    let items = enrich_items(items_pre, limit).await?;
    Ok((items, title, description))
}
