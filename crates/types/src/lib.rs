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

/// Logical entry view used by first-party clients to switch between
/// article / picture / video / etc. timelines.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryView {
    /// Do not apply any view-based filtering.
    All,
    /// Traditional article-centric timeline (default view).
    Articles,
    /// Image-heavy timeline.
    Pictures,
    /// Video-centric timeline.
    Videos,
    /// Audio/podcast timeline.
    Audios,
    /// Social/short-form timeline.
    Social,
    /// Notification/alert-style timeline.
    Notifications,
}

impl EntryView {
    /// Serialize to the canonical snake_case string representation used in
    /// the database and over the wire.
    pub fn as_str(self) -> &'static str {
        match self {
            EntryView::All => "all",
            EntryView::Articles => "articles",
            EntryView::Pictures => "pictures",
            EntryView::Videos => "videos",
            EntryView::Audios => "audios",
            EntryView::Social => "social",
            EntryView::Notifications => "notifications",
        }
    }

    /// Parse from a snake_case string, returning `None` for unknown values.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "all" => Some(EntryView::All),
            "articles" => Some(EntryView::Articles),
            "pictures" => Some(EntryView::Pictures),
            "videos" => Some(EntryView::Videos),
            "audios" => Some(EntryView::Audios),
            "social" => Some(EntryView::Social),
            "notifications" => Some(EntryView::Notifications),
            _ => None,
        }
    }
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
    pub view: Option<EntryView>,
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
    pub view: Option<EntryView>,
}

/// Generic `{ id }` response used by multiple create endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdResp {
    pub id: i64,
}

/// Built-in view descriptor used by `/api/v1/views`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewDto {
    /// Logical key identifying the view (snake_case string on the wire).
    pub key: EntryView,
    /// Human-friendly label suitable for UI.
    pub label: String,
    /// Optional description to explain when this view is useful.
    pub description: Option<String>,
}

/// Filters payload used by smart views and stored as JSON.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SmartViewFiltersDto {
    pub feed_ids: Option<Vec<i64>>,
    pub category_ids: Option<Vec<i64>>,
    pub label_ids: Option<Vec<i64>>,
    pub search: Option<String>,
    /// Optional status filter: "read" | "unread" | "starred".
    pub status: Option<String>,
}

/// Smart view descriptor used by `/api/v1/smart-views`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartViewDto {
    pub id: i64,
    pub name: String,
    pub view: EntryView,
    pub filters: SmartViewFiltersDto,
    pub sort_by: Option<String>,
    pub sort_order: Option<String>,
    pub pinned: bool,
}
