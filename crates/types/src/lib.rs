//! Shared DTO and query types for Captura APIs.
//! This crate is dependency-light and can be reused
//! by first-party clients (TUI/CLI/GUI) to keep
//! request/response types in sync with the server.

use serde::{Deserialize, Deserializer, Serialize};

/// Common paging parameters for list endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Paging {
    #[serde(default, deserialize_with = "deserialize_opt_u64")]
    pub limit: Option<u64>,
    #[serde(default, deserialize_with = "deserialize_opt_u64")]
    pub offset: Option<u64>,
}

fn deserialize_opt_u64<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum MaybeU64 {
        Num(u64),
        Str(String),
    }

    let value = Option::<MaybeU64>::deserialize(deserializer)?;
    match value {
        None => Ok(None),
        Some(MaybeU64::Num(n)) => Ok(Some(n)),
        Some(MaybeU64::Str(s)) => {
            let s = s.trim();
            if s.is_empty() {
                Ok(None)
            } else {
                s.parse::<u64>().map(Some).map_err(serde::de::Error::custom)
            }
        }
    }
}

/// Common sorting parameters for list endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sorting {
    pub sort_by: Option<String>,
    pub order: Option<String>,
}

/// Logical entry view used by first-party clients to switch between
/// article / picture / video / etc. timelines.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
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
    #[allow(clippy::should_implement_trait)]
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

    /// Decode from an optional database string column.
    ///
    /// This is a small convenience wrapper around `from_str` so that callers
    /// don't have to repeat `as_deref().and_then(EntryView::from_str)` in
    /// every handler.
    pub fn from_db(raw: Option<&str>) -> Option<Self> {
        raw.and_then(Self::from_str)
    }

    /// Encode into the string representation stored in the database.
    pub fn to_db(self) -> String {
        self.as_str().to_string()
    }

    /// Compute the effective view for a feed when both feed-level and
    /// category-level views are available.
    ///
    /// Semantics:
    /// - If `feed_view` is set and valid, it wins;
    /// - Otherwise, if `category_view` is set and valid, it is used;
    /// - Otherwise, fall back to the default article-centric view.
    pub fn effective(feed_view: Option<&str>, category_view: Option<&str>) -> Self {
        if let Some(v) = Self::from_db(feed_view) {
            v
        } else if let Some(v) = Self::from_db(category_view) {
            v
        } else {
            EntryView::Articles
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
    /// Preferred view for this feed. This is always a concrete `EntryView`
    /// value on the wire; when the server had no explicit preference stored
    /// it will default to `EntryView::Articles`.
    pub view: EntryView,
    /// Optional favicon identifier associated with this feed.
    /// When present, the corresponding binary can be fetched via
    /// `/api/v1/favicons/{favicon_id}`.
    pub favicon_id: Option<i64>,
    /// Number of parsing/fetch errors recorded for this feed.
    /// This mirrors `feed.error_count` and is primarily intended for
    /// first-party UIs to display error badges.
    pub error_count: i32,
    /// Last parsing/fetch error message, if any.
    pub last_error_message: Option<String>,
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
    /// Optional list of tag names associated with this entry for the current user.
    /// For performance reasons, listing endpoints such as `/api/v1/entries`
    /// may omit this field; it is primarily populated by `/api/v1/entries/{id}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
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
    /// Preferred view for this category; feeds can inherit from it when
    /// created without an explicit view. Always non-null on the wire.
    pub view: EntryView,
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

/// Summary counters per view, used by `/api/v1/views/summary`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewSummaryDto {
    pub view: EntryView,
    pub feed_count: i64,
    pub unread_count: i64,
}

/// Unified timeline descriptor used by `/api/v1/timelines`.
///
/// This surface merges built-in views (Articles/Pictures/...) and user-defined
/// smart views into a single list that clients can present as “timelines”.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineDto {
    /// Timeline kind, e.g. "view" or "smart_view".
    pub kind: String,
    /// Smart view id for kind="smart_view"; null for built-in view timelines.
    pub id: Option<i64>,
    /// Logical view used by this timeline.
    pub view: EntryView,
    /// Human-friendly name of the timeline.
    pub name: String,
    /// Optional description suitable for tooltips or settings.
    pub description: Option<String>,
    /// Whether this timeline is pinned/highlighted in UI (for smart views).
    pub pinned: bool,
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

/// Captura-native, view-aware JSON export payload used by
/// `/api/v1/export/full` and `/api/v1/import/full`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FullExport {
    pub version: String,
    pub exported_at: String,
    pub categories: Vec<ExportCategory>,
    pub feeds: Vec<ExportFeed>,
    pub smart_views: Vec<ExportSmartView>,
    /// Optional label snapshot for the current user. Older payloads may omit this
    /// field; consumers should treat missing/empty as "no labels exported".
    #[serde(default)]
    pub labels: Vec<ExportLabel>,
    /// Optional user preference snapshot for the current user, modeled as a list
    /// of key/value pairs. Missing/empty means "no prefs exported".
    #[serde(default)]
    pub user_prefs: Vec<ExportUserPref>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportCategory {
    pub id: i64,
    pub name: String,
    /// Preferred view for this category. Exposed as `EntryView` on the wire.
    pub view: EntryView,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportFeed {
    pub id: i64,
    pub title: Option<String>,
    pub site_url: Option<String>,
    pub feed_url: String,
    pub category_id: Option<i64>,
    /// Preferred view for this feed; same key space as `ExportCategory.view`.
    pub view: EntryView,
    pub r#type: String,
    pub fetch: ExportFeedFetch,
    pub filters: ExportFeedFilters,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportFeedFetch {
    pub user_agent: Option<String>,
    pub headers_json: Option<serde_json::Value>,
    pub cookies: Option<String>,
    pub proxy_url: Option<String>,
    pub fetch_via_proxy: bool,
    pub disable_http2: bool,
    pub allow_invalid_certs: bool,
    pub request_timeout_ms: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportFeedFilters {
    pub scraper_rules: Option<String>,
    pub rewrite_rules: Option<String>,
    pub blocklist_rules: Option<String>,
    pub keeplist_rules: Option<String>,
    pub url_rewrite_rules: Option<String>,
    pub block_filter_entry_rules: Option<String>,
    pub keep_filter_entry_rules: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportSmartView {
    pub id: i64,
    pub name: String,
    /// Logical view used by this smart view timeline.
    pub view: EntryView,
    pub filters: serde_json::Value,
    pub sort_by: Option<String>,
    pub sort_order: Option<String>,
    pub pinned: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportLabel {
    pub id: i64,
    pub name: String,
    pub color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportUserPref {
    pub key: String,
    pub value: Option<serde_json::Value>,
}
