//! Common utilities and types shared across workspace.
//! Keep this crate minimal and dependency-light.

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("configuration error: {0}")]
    Config(String),
    #[error("network error: {0}")]
    Network(String),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EnclosureType {
    Audio,
    Video,
    Image,
    File,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Enclosure {
    pub url: String,
    pub r#type: Option<String>,
    pub length: Option<i64>,
    pub kind: Option<EnclosureType>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedEntry {
    pub guid: Option<String>,
    pub url: Option<String>,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub content_html: Option<String>,
    pub author: Option<String>,
    pub published_at: Option<chrono::DateTime<chrono::Utc>>,
    pub enclosures: Vec<Enclosure>,
    pub extras: serde_json::Value,
}

/// 从 HTML 文本中去除标签，保留纯文本（简单实现）
pub fn strip_html_simple(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ => {
                if !in_tag {
                    out.push(ch);
                }
            }
        }
    }
    out
}

/// 根据 HTML 正文估算阅读时长（分钟）。
pub fn reading_time_minutes_from_html(html: &str, wpm: usize) -> i32 {
    let text = strip_html_simple(html);
    let wpm = wpm.max(50);
    let words = text.split_whitespace().count();
    std::cmp::max(1, words.div_ceil(wpm)) as i32
}
