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
