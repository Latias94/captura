use askama::Template;
use axum::{
    extract::Query,
    http::HeaderMap,
    response::{Html, IntoResponse, Redirect},
};
use serde::Deserialize;
use std::time::Duration;

use crate::filters;
use crate::i18n;
use crate::util::{
    api_base, gen_csp_nonce, http_client, load_snippets, read_token_cookie, resolve_lang,
};

#[derive(Deserialize, Clone)]
#[allow(dead_code)]
pub struct UiFeedDto {
    pub id: i64,
    pub title: Option<String>,
    pub site_url: Option<String>,
    pub unread_count: Option<i64>,
    pub category: Option<UiCategory>,
    #[serde(default)]
    pub parsing_error_count: Option<i32>,
    #[serde(default)]
    pub parsing_error_message: Option<String>,
}

#[derive(Deserialize, Clone)]
pub struct UiSmartView {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub view: Option<String>,
    #[serde(default)]
    pub filters: Option<UiSmartViewFilters>,
}

#[derive(Deserialize, Clone, Default)]
#[allow(dead_code)]
pub struct UiSmartViewFilters {
    #[serde(default)]
    pub feed_ids: Option<Vec<i64>>,
    #[serde(default)]
    pub category_ids: Option<Vec<i64>>,
    #[serde(default)]
    pub label_ids: Option<Vec<i64>>,
    #[serde(default)]
    pub search: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Deserialize, Clone)]
pub struct UiCategory {
    pub id: i64,
    pub title: String,
    #[serde(default)]
    pub feed_count: Option<i64>,
    #[serde(default)]
    pub total_unread: Option<i64>,
}

#[derive(Template)]
#[template(path = "feeds.html")]
pub struct FeedsPage<'a> {
    pub title: &'a str,
    pub feeds: &'a [UiFeedDto],
    pub categories: &'a [UiCategory],
    pub smart_views: &'a [UiSmartView],
    pub selected_category: Option<i64>,
    pub has_uncategorized: bool,
    pub dict: &'a std::collections::HashMap<String, String>,
    pub csp_nonce: &'a str,
    pub custom_css: &'a str,
    pub custom_js: &'a str,
    pub external_font_hosts: &'a str,
}

#[derive(Deserialize, Default)]
pub struct UiFeedsQuery {
    pub category_id: Option<i64>,
}

pub async fn ui_feeds(headers: HeaderMap, Query(fq): Query<UiFeedsQuery>) -> impl IntoResponse {
    let Some(token) = read_token_cookie(&headers) else {
        return Redirect::to("/login").into_response();
    };
    let lang = resolve_lang(&headers).await;
    let dict = i18n::load(&lang);
    let snippets = load_snippets(&headers).await;
    let nonce = gen_csp_nonce();
    let Some(cli) = http_client(3) else {
        return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "http client error",
        )
            .into_response();
    };
    let mut url = format!("{}/v1/feeds?withCounters=true", api_base());
    let mut selected_category = None;
    if let Some(cid) = fq.category_id {
        url.push_str(&format!("&category_id={}", cid));
        selected_category = Some(cid);
    }
    let res = cli
        .get(url)
        .header("X-Auth-Token", token.clone())
        .send()
        .await;
    let feeds: Vec<UiFeedDto> = match res.and_then(|r| r.error_for_status()) {
        Ok(resp) => resp.json().await.unwrap_or_default(),
        Err(_) => Vec::new(),
    };
    let has_uncategorized = feeds.iter().any(|f| f.category.is_none());
    // categories for dropdown (ignore extra fields)
    let cats_url = format!("{}/v1/categories?counts=true", api_base());
    let res2 = cli
        .get(cats_url)
        .header("X-Auth-Token", token.clone())
        .send()
        .await;
    let categories: Vec<UiCategory> = match res2.and_then(|r| r.error_for_status()) {
        Ok(resp) => resp.json().await.unwrap_or_default(),
        Err(_) => Vec::new(),
    };
    // smart views (native /api/v1)
    let sv_url = format!("{}/api/v1/smart-views", api_base());
    let res3 = cli
        .get(sv_url)
        .header(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {}", token),
        )
        .send()
        .await;
    let smart_views: Vec<UiSmartView> = match res3.and_then(|r| r.error_for_status()) {
        Ok(resp) => resp.json().await.unwrap_or_default(),
        Err(_) => Vec::new(),
    };
    let tpl = FeedsPage {
        title: "Feeds",
        feeds: &feeds,
        categories: &categories,
        smart_views: &smart_views,
        selected_category,
        has_uncategorized,
        dict: &dict,
        csp_nonce: &nonce,
        custom_css: &snippets.custom_css,
        custom_js: &snippets.custom_js,
        external_font_hosts: &snippets.external_font_hosts,
    };
    match tpl.render() {
        Ok(s) => Html(s).into_response(),
        Err(_) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "template error",
        )
            .into_response(),
    }
}
