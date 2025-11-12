use captura_storage::entity::{category, feed};
use serde::Serialize;

#[derive(Serialize)]
pub struct MfCategoryDto {
    pub id: i64,
    pub title: String,
}

#[derive(Serialize)]
pub struct MfFeedDto {
    pub id: i64,
    #[serde(rename = "user_id")]
    pub user_id: i64,
    pub feed_url: String,
    pub site_url: Option<String>,
    pub title: Option<String>,
    #[serde(rename = "checked_at", skip_serializing_if = "Option::is_none")]
    pub checked_at: Option<String>,
    #[serde(rename = "etag_header", skip_serializing_if = "Option::is_none")]
    pub etag_header: Option<String>,
    #[serde(
        rename = "last_modified_header",
        skip_serializing_if = "Option::is_none"
    )]
    pub last_modified_header: Option<String>,
    #[serde(
        rename = "parsing_error_message",
        skip_serializing_if = "Option::is_none"
    )]
    pub parsing_error_message: Option<String>,
    #[serde(rename = "parsing_error_count")]
    pub parsing_error_count: i32,
    pub disabled: bool,
    #[serde(rename = "ignore_http_cache")]
    pub ignore_http_cache: bool,
    #[serde(rename = "allow_self_signed_certificates")]
    pub allow_self_signed_certificates: bool,
    #[serde(rename = "fetch_via_proxy")]
    pub fetch_via_proxy: bool,
    #[serde(rename = "scraper_rules", skip_serializing_if = "Option::is_none")]
    pub scraper_rules: Option<String>,
    #[serde(rename = "rewrite_rules", skip_serializing_if = "Option::is_none")]
    pub rewrite_rules: Option<String>,
    #[serde(rename = "urlrewrite_rules", skip_serializing_if = "Option::is_none")]
    pub urlrewrite_rules: Option<String>,
    #[serde(rename = "blocklist_rules", skip_serializing_if = "Option::is_none")]
    pub blocklist_rules: Option<String>,
    #[serde(rename = "keeplist_rules", skip_serializing_if = "Option::is_none")]
    pub keeplist_rules: Option<String>,
    #[serde(
        rename = "block_filter_entry_rules",
        skip_serializing_if = "Option::is_none"
    )]
    pub block_filter_entry_rules: Option<String>,
    #[serde(
        rename = "keep_filter_entry_rules",
        skip_serializing_if = "Option::is_none"
    )]
    pub keep_filter_entry_rules: Option<String>,
    #[serde(rename = "crawler")]
    pub crawler: bool,
    #[serde(rename = "user_agent", skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
    #[serde(rename = "cookie", skip_serializing_if = "Option::is_none")]
    pub cookie: Option<String>,
    #[serde(rename = "username", skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(rename = "password", skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<MfCategoryDto>,
    #[serde(rename = "hide_globally")]
    pub hide_globally: bool,
    #[serde(rename = "disable_http2")]
    pub disable_http2: bool,
    #[serde(rename = "proxy_url", skip_serializing_if = "Option::is_none")]
    pub proxy_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unread_count: Option<i64>,
}

pub fn map_feed(f: feed::Model, cat: Option<category::Model>) -> MfFeedDto {
    MfFeedDto {
        id: f.id,
        user_id: f.user_id,
        feed_url: f.feed_url,
        site_url: f.site_url,
        title: f.title,
        checked_at: f.checked_at.map(|d| d.to_rfc3339()),
        etag_header: f.etag,
        last_modified_header: f.last_modified,
        parsing_error_message: None,
        parsing_error_count: f.error_count,
        disabled: f.disabled,
        ignore_http_cache: false,
        allow_self_signed_certificates: f.allow_invalid_certs,
        fetch_via_proxy: f.fetch_via_proxy,
        scraper_rules: f.scraper_rules,
        rewrite_rules: f.rewrite_rules,
        urlrewrite_rules: f.url_rewrite_rules,
        blocklist_rules: f.blocklist_rules,
        keeplist_rules: f.keeplist_rules,
        block_filter_entry_rules: f.block_filter_entry_rules,
        keep_filter_entry_rules: f.keep_filter_entry_rules,
        crawler: false,
        user_agent: f.user_agent,
        cookie: f.cookies,
        username: None,
        password: None,
        category: cat.map(|c| MfCategoryDto {
            id: c.id,
            title: c.name,
        }),
        hide_globally: false,
        disable_http2: f.disable_http2,
        proxy_url: f.proxy_url,
        unread_count: None,
    }
}

#[derive(Serialize, Clone)]
pub struct MfEnclosureDto {
    pub id: i64,
    pub url: String,
    #[serde(rename = "mime_type")]
    pub mime_type: String,
    pub size: i64,
    #[serde(rename = "media_progression")]
    pub media_progression: i64,
}

#[derive(Serialize)]
pub struct MfEntryDto {
    pub id: i64,
    #[serde(rename = "published_at")]
    pub date: Option<String>,
    #[serde(rename = "changed_at")]
    pub changed_at: Option<String>,
    #[serde(rename = "created_at")]
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feed: Option<MfFeedDto>,
    pub hash: Option<String>,
    pub url: Option<String>,
    #[serde(rename = "comments_url")]
    pub comments_url: Option<String>,
    pub title: Option<String>,
    pub status: String,
    pub content: Option<String>,
    pub author: Option<String>,
    #[serde(rename = "share_code")]
    pub share_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enclosures: Option<Vec<MfEnclosureDto>>,
    pub tags: Vec<String>,
    #[serde(rename = "reading_time")]
    pub reading_time: i32,
    #[serde(rename = "user_id")]
    pub user_id: i64,
    #[serde(rename = "feed_id")]
    pub feed_id: i64,
    pub starred: bool,
}

#[derive(Serialize)]
pub struct MfEntryResultSet {
    pub total: i64,
    pub entries: Vec<MfEntryDto>,
}
