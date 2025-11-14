//! Rules DSL v1 schema and validator.
//!
//! This module reflects `docs/rules-dsl.md` and is the
//! recommended surface for new rules. The engine may initially
//! implement only a subset of the schema; fields are kept
//! flexible for forwards compatibility.

use captura_common::{Error, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use tracing::instrument;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleSpecV1 {
    pub id: String,
    #[serde(default = "default_version")]
    pub version: i32,
    pub description: Option<String>,
    pub author: Option<String>,
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub examples: Vec<String>,
    #[serde(rename = "match")]
    pub match_spec: Option<MatchSpec>,
    pub params: Option<ParamsSpec>,
    #[serde(default)]
    pub fetch: FetchDefaults,
    pub source: SourceSpec,
    pub filters: Option<FiltersSpec>,
    pub transform: Option<TransformSpec>,
}

fn default_version() -> i32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchSpec {
    pub url: Option<MatchUrlSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchUrlSpec {
    pub host: Option<String>,
    pub path_regex: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ParamsSpec {
    #[serde(default)]
    pub defaults: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    pub docs: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FetchDefaults {
    pub user_agent: Option<String>,
    pub timeout_ms: Option<u64>,
    pub smart: Option<bool>,
    pub respect_robots: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    ListDetail,
    SinglePage,
    Json,
    XPath,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceSpec {
    #[serde(rename = "type")]
    pub kind: SourceType,

    // list_detail
    pub list: Option<ListSourceSpec>,
    pub content: Option<ContentSpec>,

    // single_page / json / xpath
    pub request: Option<RequestSpec>,

    // json
    pub root: Option<String>,
    pub mapping: Option<JsonMappingSpec>,
    pub from_html: Option<FromHtmlSpec>,

    // xpath
    pub xpath: Option<XPathSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RequestSpec {
    pub url: String,
    pub method: Option<String>,
    pub headers: Option<serde_json::Map<String, serde_json::Value>>,
    pub body: Option<serde_json::Value>,
    pub timeout_ms: Option<u64>,
    pub smart: Option<bool>,
    pub respect_robots: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ListSourceSpec {
    pub request: RequestSpec,
    pub item: String,
    pub link: Option<String>,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub published_at: Option<TimestampSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentSpec {
    #[serde(default = "default_content_mode")]
    pub mode: ContentMode,
    pub selector: Option<String>,
    #[serde(default)]
    pub remove: Vec<String>,
    pub fallback: Option<ContentFallback>,
    pub use_entry_url: Option<bool>,
}

fn default_content_mode() -> ContentMode {
    ContentMode::Css
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentMode {
    Css,
    Readability,
    JsonFragment,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentFallback {
    None,
    Summary,
    WholePage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimestampSpec {
    pub selector: Option<String>,
    pub format: Option<String>,
    #[serde(rename = "path")]
    pub path: Option<String>, // used in JSON/xpath variants
    pub expr: Option<String>, // xpath variant
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct JsonMappingSpec {
    pub title: Option<String>,
    pub url: Option<String>,
    pub summary: Option<String>,
    pub content_html: Option<String>,
    pub author: Option<String>,
    pub published_at: Option<JsonTimestampMapping>,
    pub enclosure: Option<JsonEnclosureMapping>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct JsonTimestampMapping {
    pub path: String,
    pub format: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct JsonEnclosureMapping {
    pub url: Option<String>,
    pub r#type: Option<String>,
    pub length: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FromHtmlSpec {
    pub request: Option<RequestSpec>,
    pub selector: String,
    pub multiple: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XPathSpec {
    pub item: String,
    pub title: Option<String>,
    pub url: Option<String>,
    pub content_html: Option<String>,
    pub published_at: Option<TimestampSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FiltersSpec {
    pub entry_include: Option<Vec<String>>,
    pub entry_exclude: Option<Vec<String>>,
    pub fetch_full_content_when: Option<Vec<FullContentCondition>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FullContentCondition {
    pub field: String, // title | summary | content_html
    pub regex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TransformSpec {
    pub url_rewrite: Option<Vec<String>>,
    pub content_rewrite: Option<Vec<String>>,
    pub content_remove_selectors: Option<Vec<String>>,
    pub content_merge: Option<ContentMergeSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentMergeSpec {
    pub mode: Option<ContentMergeMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentMergeMode {
    Replace,
    Prepend,
    Append,
}

#[instrument]
pub fn parse_rule_v1(yaml: &str) -> Result<RuleSpecV1> {
    let spec: RuleSpecV1 = serde_yaml::from_str(yaml).map_err(|e| Error::Parse(e.to_string()))?;
    validate_v1(&spec)?;
    Ok(spec)
}

#[instrument]
pub fn validate_v1(spec: &RuleSpecV1) -> Result<()> {
    if spec.id.trim().is_empty() {
        return Err(Error::Parse("rule id is required".into()));
    }
    if spec.version != 1 {
        return Err(Error::Parse(format!(
            "unsupported rule version: {} (expected 1)",
            spec.version
        )));
    }
    // Basic source validation (lightweight, engine does deeper checks).
    match spec.source.kind {
        SourceType::ListDetail => {
            if spec.source.list.is_none() {
                return Err(Error::Parse(
                    "source.list is required for type=list_detail".into(),
                ));
            }
            if spec.source.content.is_none() {
                return Err(Error::Parse(
                    "source.content is required for type=list_detail".into(),
                ));
            }
        }
        SourceType::SinglePage => {
            if spec.source.request.is_none() {
                return Err(Error::Parse(
                    "source.request is required for type=single_page".into(),
                ));
            }
            if spec.source.content.is_none() {
                return Err(Error::Parse(
                    "source.content is required for type=single_page".into(),
                ));
            }
        }
        SourceType::Json => {
            if spec.source.root.is_none() {
                return Err(Error::Parse(
                    "source.root is required for type=json".into(),
                ));
            }
            if spec.source.mapping.is_none() {
                return Err(Error::Parse(
                    "source.mapping is required for type=json".into(),
                ));
            }
            if spec.source.request.is_none() && spec.source.from_html.is_none() {
                return Err(Error::Parse(
                    "either source.request or source.from_html is required for type=json".into(),
                ));
            }
        }
        SourceType::XPath => {
            if spec.source.request.is_none() {
                return Err(Error::Parse(
                    "source.request is required for type=xpath".into(),
                ));
            }
            if spec.source.xpath.is_none() {
                return Err(Error::Parse(
                    "source.xpath is required for type=xpath".into(),
                ));
            }
        }
    }

    // Validate regexes in filters.
    if let Some(filters) = &spec.filters {
        if let Some(include) = &filters.entry_include {
            for rx in include {
                Regex::new(rx)
                    .map_err(|e| Error::Parse(format!("invalid regex in entry_include: {e}")))?;
            }
        }
        if let Some(exclude) = &filters.entry_exclude {
            for rx in exclude {
                Regex::new(rx)
                    .map_err(|e| Error::Parse(format!("invalid regex in entry_exclude: {e}")))?;
            }
        }
        if let Some(conds) = &filters.fetch_full_content_when {
            for c in conds {
                match c.field.as_str() {
                    "title" | "summary" | "content_html" => {}
                    other => {
                        return Err(Error::Parse(format!(
                            "invalid field in fetch_full_content_when: {other}"
                        )));
                    }
                }
                Regex::new(&c.regex).map_err(|e| {
                    Error::Parse(format!(
                        "invalid regex in fetch_full_content_when: {e}"
                    ))
                })?;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_list_detail_rule_ok() {
        let yaml = r#"id: captura.example.news
version: 1
description: Example news list
examples:
  - https://example.com/news

source:
  type: list_detail
  list:
    request:
      url: "https://example.com/news"
    item: "article.post"
    link: "a@href"
    title: "a.title"
  content:
    mode: css
    selector: "div.article-content"
"#;
        let spec = parse_rule_v1(yaml).expect("parse rule v1");
        assert_eq!(spec.id, "captura.example.news");
        assert_eq!(spec.version, 1);
        assert!(matches!(spec.source.kind, SourceType::ListDetail));
        let list = spec.source.list.as_ref().expect("list present");
        assert_eq!(list.request.url, "https://example.com/news");
        assert_eq!(list.item, "article.post");
        let content = spec.source.content.as_ref().expect("content present");
        assert_eq!(content.selector.as_deref(), Some("div.article-content"));
    }

    #[test]
    fn invalid_regex_in_filters_fails_validation_v1() {
        let yaml = r#"id: captura.invalid.regex
version: 1
source:
  type: list_detail
  list:
    request:
      url: "https://example.com"
    item: "article"
  content:
    mode: css
    selector: "article"
filters:
  entry_include:
    - "("
"#;
        let err = parse_rule_v1(yaml).unwrap_err();
        match err {
            Error::Parse(msg) => {
                assert!(
                    msg.contains("invalid regex"),
                    "unexpected parse error message: {msg}"
                );
            }
            other => panic!("expected Error::Parse, got {other:?}"),
        }
    }
}
