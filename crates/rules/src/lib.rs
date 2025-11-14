//! Rules DSL and validator for route-style content extraction.

use captura_common::{Error, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use tracing::instrument;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RuleSpec {
    pub id: String,
    pub description: Option<String>,
    pub examples: Vec<String>,
    #[serde(default)]
    pub fetch: FetchSpec,
    #[serde(default)]
    pub list: Option<ListSpec>,
    #[serde(default)]
    pub content: ContentSpec,
    #[serde(default)]
    pub filters: Option<FilterSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FetchSpec {
    pub user_agent: Option<String>,
    pub headers: Option<serde_json::Map<String, serde_json::Value>>, // simple map
    pub smart: Option<bool>,
    pub timeout_ms: Option<u64>,
    pub respect_robots: Option<bool>,
    pub delay_ms: Option<u64>,
    pub limit: Option<usize>,
    pub proxy_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListSpec {
    pub url: String,
    pub item: String,
    pub link: Option<String>,
    pub title: Option<String>,
    pub published_at: Option<TimestampSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContentSpec {
    #[serde(default = "default_use")]
    pub r#use: String, // css | readability
    pub selector: Option<String>,
    pub fallback: Option<String>,
}

fn default_use() -> String {
    "css".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimestampSpec {
    pub selector: String,
    pub format: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterSpec {
    pub include: Option<Vec<String>>, // regex
    pub exclude: Option<Vec<String>>, // regex
}

#[instrument]
pub fn parse_rule(yaml: &str) -> Result<RuleSpec> {
    let spec: RuleSpec = serde_yaml::from_str(yaml).map_err(|e| Error::Parse(e.to_string()))?;
    validate(&spec)?;
    Ok(spec)
}

#[instrument]
pub fn validate(spec: &RuleSpec) -> Result<()> {
    if spec.id.trim().is_empty() {
        return Err(Error::Parse("rule id is required".into()));
    }
    if let Some(filters) = &spec.filters {
        for rx in filters
            .include
            .iter()
            .flatten()
            .chain(filters.exclude.iter().flatten())
        {
            Regex::new(rx).map_err(|e| Error::Parse(format!("invalid regex: {e}")))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use captura_common::Error;

    #[test]
    fn parse_basic_rule_yaml_ok() {
        let yaml = r#"id: captura.route.github.trending
description: GitHub Trending repositories
examples:
  - https://github.com/trending
fetch:
  user_agent: captura/0.1
list:
  url: "https://github.com/trending?since={since}"
  item: "article.Box-row"
content:
  use: readability
"#;
        let spec = parse_rule(yaml).expect("parse rule");
        assert_eq!(spec.id, "captura.route.github.trending");
        assert_eq!(
            spec.description.as_deref(),
            Some("GitHub Trending repositories")
        );
        assert_eq!(spec.examples.len(), 1);
        assert_eq!(spec.fetch.user_agent.as_deref(), Some("captura/0.1"));
        assert!(spec.list.is_some());
        assert_eq!(spec.content.r#use, "readability");
    }

    #[test]
    fn invalid_regex_in_filters_fails_validation() {
        let yaml = r#"id: captura.invalid.regex
filters:
  include:
    - "("
"#;
        let err = parse_rule(yaml).unwrap_err();
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

    #[test]
    fn missing_optional_fields_use_defaults() {
        let yaml = "id: simple.rule\n";
        let spec = parse_rule(yaml).expect("parse rule");
        assert_eq!(spec.id, "simple.rule");
        assert!(spec.description.is_none());
        assert!(spec.examples.is_empty());
        assert!(spec.list.is_none());
        // ContentSpec should default to css mode when not specified.
        assert_eq!(spec.content.r#use, "css");
    }
}
