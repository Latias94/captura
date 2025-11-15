//! Shared DTO and query types for Captura APIs.
//! This crate is dependency-light and can be reused
//! by first-party clients (TUI/CLI/GUI) to keep
//! request/response types in sync with the server.

use serde::{Deserialize, Serialize};

/// Common paging parameters for list endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Paging {
    pub limit: Option<u64>,
    pub offset: Option<u64>,
}

/// Common sorting parameters for list endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sorting {
    pub sort_by: Option<String>,
    pub order: Option<String>,
}

/// Minimal feed representation exposed by `/api/v1/feeds`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedDto {
    pub id: i64,
    pub title: Option<String>,
    pub feed_url: String,
    pub site_url: Option<String>,
    pub disabled: bool,
    pub category_id: Option<i64>,
}

/// Minimal entry representation exposed by `/api/v1/entries` and `/api/v1/entries/{id}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryDto {
    pub id: i64,
    pub feed_id: i64,
    pub url: Option<String>,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub content_html: Option<String>,
    pub author: Option<String>,
    pub published_at: Option<String>,
    pub is_read: bool,
    pub is_starred: bool,
}

/// Entry content payload returned by `/api/v1/entries/{id}/content`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryContentDto {
    pub content_html: String,
    pub title: Option<String>,
}

/// Feed-level read/unread counters for first-party clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedCountersDto {
    pub reads: std::collections::HashMap<i64, i64>,
    pub unreads: std::collections::HashMap<i64, i64>,
}

/// Category-level unread counters. `category_id = None` means "uncategorized".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryCounterDto {
    pub category_id: Option<i64>,
    pub unread: i64,
}

/// Minimal category representation exposed by `/api/v1/categories`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryDto {
    pub id: i64,
    pub name: String,
}

/// Generic `{ id }` response used by multiple create endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdResp {
    pub id: i64,
}
