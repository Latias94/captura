use askama::Template;
use axum::{
    extract::Query,
    http::HeaderMap,
    response::{Html, IntoResponse, Redirect},
};
use serde::Deserialize;

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
    #[serde(default)]
    pub favicon_id: Option<i64>,
    pub unread_count: Option<i64>,
    pub category: Option<UiCategory>,
    #[serde(default)]
    pub parsing_error_count: Option<i32>,
    #[serde(default)]
    pub parsing_error_message: Option<String>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct ApiFeedDto {
    pub id: i64,
    pub title: Option<String>,
    pub feed_url: String,
    pub site_url: Option<String>,
    pub disabled: bool,
    pub category_id: Option<i64>,
    #[serde(default)]
    pub favicon_id: Option<i64>,
    pub error_count: i32,
    pub last_error_message: Option<String>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct ApiFeedCountersDto {
    pub reads: std::collections::HashMap<i64, i64>,
    pub unreads: std::collections::HashMap<i64, i64>,
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

#[derive(Deserialize)]
struct ApiCategoryDto {
    pub id: i64,
    pub name: String,
}

#[derive(Deserialize)]
struct ApiCategoryCounterDto {
    pub category_id: Option<i64>,
    pub unread: i64,
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
    // Fetch feeds via native `/api/v1/feeds` (view-aware) and counters via
    // `/api/v1/feeds/counters` so that the WebUI does not depend on the
    // Miniflux-compatible `/v1/feeds` endpoint for listing.
    let mut url = format!("{}/api/v1/feeds?sort_by=title&order=asc", api_base());
    let mut selected_category = None;
    if let Some(cid) = fq.category_id {
        url.push_str(&format!("&category_id={}", cid));
        selected_category = Some(cid);
    }
    let res_feeds = cli
        .get(&url)
        .header(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {}", token.clone()),
        )
        .send()
        .await;
    let api_feeds: Vec<ApiFeedDto> = match res_feeds.and_then(|r| r.error_for_status()) {
        Ok(resp) => resp.json().await.unwrap_or_default(),
        Err(_) => Vec::new(),
    };
    let counters_url = format!("{}/api/v1/feeds/counters", api_base());
    let res_cnt = cli
        .get(&counters_url)
        .header(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {}", token.clone()),
        )
        .send()
        .await;
    let mut unread_map: std::collections::HashMap<i64, i64> = std::collections::HashMap::new();
    if let Ok(resp) = res_cnt.and_then(|r| r.error_for_status()) {
        if let Ok(fc) = resp.json::<ApiFeedCountersDto>().await {
            unread_map = fc.unreads;
        }
    }
    // categories for dropdown (ignore extra fields)
    let cats_url = format!("{}/api/v1/categories", api_base());
    let res2 = cli
        .get(&cats_url)
        .header(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {}", token.clone()),
        )
        .send()
        .await;
    let api_categories: Vec<ApiCategoryDto> = match res2.and_then(|r| r.error_for_status()) {
        Ok(resp) => resp.json().await.unwrap_or_default(),
        Err(_) => Vec::new(),
    };
    let cats_cnt_url = format!("{}/api/v1/categories/counters", api_base());
    let res3 = cli
        .get(&cats_cnt_url)
        .header(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {}", token.clone()),
        )
        .send()
        .await;
    let mut cat_unreads: std::collections::HashMap<Option<i64>, i64> =
        std::collections::HashMap::new();
    if let Ok(resp) = res3.and_then(|r| r.error_for_status()) {
        if let Ok(list) = resp.json::<Vec<ApiCategoryCounterDto>>().await {
            for c in list {
                cat_unreads.insert(c.category_id, c.unread);
            }
        }
    }
    // Compute feed_count per category id.
    let mut feed_counts: std::collections::HashMap<i64, i64> = std::collections::HashMap::new();
    for f in &api_feeds {
        if let Some(cid) = f.category_id {
            *feed_counts.entry(cid).or_insert(0) += 1;
        }
    }
    let mut categories: Vec<UiCategory> = Vec::new();
    for c in api_categories {
        let feed_count = feed_counts.get(&c.id).copied();
        let total_unread = cat_unreads.get(&Some(c.id)).copied();
        categories.push(UiCategory {
            id: c.id,
            title: c.name,
            feed_count,
            total_unread,
        });
    }
    // Map feeds to UiFeedDto, attaching a shallow category object for
    // template grouping and unread/error badges.
    let mut feeds: Vec<UiFeedDto> = Vec::new();
    for f in api_feeds {
        let unread = unread_map.get(&f.id).copied();
        let cat = f.category_id.and_then(|cid| {
            categories.iter().find(|c| c.id == cid).map(|c| UiCategory {
                id: c.id,
                title: c.title.clone(),
                feed_count: None,
                total_unread: None,
            })
        });
        feeds.push(UiFeedDto {
            id: f.id,
            title: f.title,
            site_url: f.site_url,
            favicon_id: f.favicon_id,
            unread_count: unread,
            category: cat,
            parsing_error_count: Some(f.error_count),
            parsing_error_message: f.last_error_message,
        });
    }
    let has_uncategorized = feeds.iter().any(|f| f.category.is_none());
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
