#![allow(dead_code)]
use serde::Deserialize;

// Fever-compatible endpoint query (lean implementation: mostly read-only)
#[derive(Deserialize)]
pub(crate) struct FeverQuery {
    pub api: Option<i32>,
    pub api_key: Option<String>,
    pub groups: Option<i32>,
    pub feeds: Option<i32>,
    pub favicons: Option<i32>,
    pub items: Option<i32>,
    pub since_id: Option<i64>,
    pub limit: Option<u64>,
    pub unread_item_ids: Option<i32>,
    pub saved_item_ids: Option<i32>,
    // Optional write operations
    pub mark: Option<String>, // item|feed|group|all
    #[serde(rename = "as")]
    pub r#as: Option<String>, // read|unread|saved|unsaved
    pub id: Option<String>,   // item ids (csv) or feed/group id
    pub before: Option<i64>,  // timestamp
}
