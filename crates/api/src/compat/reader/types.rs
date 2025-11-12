#![allow(dead_code)]
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub(crate) struct ReaderQuery {
    pub n: Option<u64>,
    pub s: Option<String>,
    pub c: Option<String>,
    pub q: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct ReaderSubscriptionCategory {
    pub id: String,
    pub label: String,
}

#[derive(Serialize)]
pub(crate) struct ReaderSubscriptionItem {
    pub id: String,
    pub title: String,
    pub categories: Vec<ReaderSubscriptionCategory>,
    pub url: String,
    #[serde(rename = "htmlUrl")]
    pub html_url: Option<String>,
    #[serde(rename = "iconUrl")]
    pub icon_url: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct ReaderSubscriptionListResp {
    pub subscriptions: Vec<ReaderSubscriptionItem>,
}

#[derive(Serialize)]
pub(crate) struct ReaderOrigin {
    #[serde(rename = "streamId")]
    pub stream_id: String,
    pub title: Option<String>,
    #[serde(rename = "htmlUrl")]
    pub html_url: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct ReaderLink {
    pub href: String,
    pub r#type: &'static str,
}

#[derive(Serialize)]
pub(crate) struct ReaderContent {
    pub content: String,
}

#[derive(Serialize)]
pub(crate) struct ReaderItem {
    pub id: String,
    pub title: Option<String>,
    pub published: i64,
    pub updated: i64,
    #[serde(rename = "crawlTimeMsec")]
    pub crawl_time_msec: String,
    pub categories: Vec<String>,
    pub alternate: Vec<ReaderLink>,
    pub origin: ReaderOrigin,
    pub author: Option<String>,
    pub summary: Option<ReaderContent>,
    pub content: Option<ReaderContent>,
}

#[derive(Serialize)]
pub(crate) struct ReaderStreamResp {
    pub items: Vec<ReaderItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub continuation: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct ReaderItemsIdsQuery {
    pub n: Option<u64>,
    pub s: Option<String>,
    pub c: Option<String>,
    pub xt: Option<String>,
    pub q: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct ReaderItemRef {
    pub id: String,
    #[serde(rename = "directStreamIds")]
    pub direct_stream_ids: Vec<String>,
    #[serde(rename = "timestampUsec")]
    pub timestamp_usec: String,
}

#[derive(Serialize)]
pub(crate) struct ReaderItemsIdsResp {
    #[serde(rename = "itemRefs")]
    pub item_refs: Vec<ReaderItemRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub continuation: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct ReaderItemsContentsQuery {
    pub n: Option<u64>,
    pub s: Option<String>,
    pub c: Option<String>,
    pub q: Option<String>,
    pub xt: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct ReaderItemsContentsItem {
    pub id: String,
    pub title: Option<String>,
    pub categories: Vec<String>,
    #[serde(rename = "alternate")]
    pub alternate: Vec<ReaderLink>,
    pub origin: ReaderOrigin,
    pub author: Option<String>,
    pub summary: Option<ReaderContent>,
    pub content: Option<ReaderContent>,
}

#[derive(Serialize)]
pub(crate) struct ReaderItemsContentsResp {
    pub items: Vec<ReaderItemsContentsItem>,
}

#[derive(Deserialize)]
pub(crate) struct ReaderEditTagForm {
    pub a: Option<String>,
    pub r: Option<String>,
    pub i: String,
}

#[derive(Deserialize)]
pub(crate) struct ReaderMarkAllForm {
    pub s: String,
    pub t: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct ReaderUnreadCountItem {
    pub id: String,
    pub count: i64,
}

#[derive(Serialize)]
pub(crate) struct ReaderUnreadCountResp {
    pub unreadcounts: Vec<ReaderUnreadCountItem>,
}

#[derive(Deserialize)]
pub(crate) struct ReaderQuickAddForm {
    pub quickadd: String,
}

#[derive(Serialize)]
pub(crate) struct ReaderQuickAddResp {
    #[serde(rename = "numResults")]
    pub num_results: i32,
    #[serde(rename = "streamId")]
    pub stream_id: String,
    pub query: String,
}

#[derive(Deserialize)]
pub(crate) struct ReaderSubEditForm {
    pub ac: String,
    pub s: String,
}
