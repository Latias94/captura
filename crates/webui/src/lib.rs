//! Minimal SSR Web UI for Captura (Askama-based).
//! This crate exposes a Router that can be mounted by the API service.
//! It deliberately avoids frontend frameworks and large inline scripts/styles
//! and keeps CSP strict by using nonces for small user-provided CSS/JS.

use askama::Template;
use axum::{
    body::Bytes,
    extract::Path,
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
    Router,
};
use serde::Deserialize;

mod static_assets;
use static_assets::static_handler;
mod i18n;
mod pages_entries;
mod pages_feeds;
mod pages_hub;
mod pages_index;
mod pages_settings;
mod util;
use util::{api_base, gen_csp_nonce, load_snippets, read_token_cookie, resolve_lang};

// ----- Templates -----

// Askama looks for `crate::filters` by default when compiling templates.
// Provide an empty module to satisfy the import (we only use built-ins).
pub mod filters {
    // Provide a length filter for templates using `| length`
    pub fn length<T>(v: &[T]) -> ::askama::Result<usize> {
        Ok(v.len())
    }
    // Compare i64 and Option<i64> for equality (used by category dropdown selection)
    pub fn eq64(v: &i64, other: &Option<i64>) -> ::askama::Result<bool> {
        Ok(other.is_some_and(|x| x == *v))
    }
    // Check whether Option<i32> is > 0 (used for error badge visibility)
    pub fn gt0_i32(v: &Option<i32>) -> ::askama::Result<bool> {
        Ok(v.is_some_and(|n| n > 0))
    }
    // Check whether a string is non-empty (used when deciding CSP external_font_hosts)
    pub fn non_empty_str(v: &str) -> ::askama::Result<bool> {
        Ok(!v.is_empty())
    }
}

/// Build a UI router with generic state.
/// No handler here takes a dependency on the state so S can be any shared state.
pub fn router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/", get(pages_index::index))
        .route("/login", get(pages_index::login))
        .route("/signup", get(pages_index::signup))
        .route("/settings", get(pages_settings::ui_settings))
        .route("/hub", get(pages_hub::ui_hub_routes))
        .route("/hub/test", get(pages_hub::ui_hub_test))
        .route(
            "/rules/test",
            post(pages_hub::ui_rules_test).get(|headers: HeaderMap| async move {
                // initial empty form render on GET
                let lang = resolve_lang(&headers).await;
                let dict = i18n::load(&lang);
                let snippets = load_snippets(&headers).await;
                let nonce = gen_csp_nonce();
                let empty = String::new();
                let none_yaml: Option<String> = None;
                let none_result: Option<pages_hub::UiTryRuleResp> = None;
                let tpl = pages_hub::RulesTestPage {
                    title: "Test Rule",
                    url: &empty,
                    yaml: &none_yaml,
                    result: &none_result,
                    dict: &dict,
                    csp_nonce: &nonce,
                    custom_css: &snippets.custom_css,
                    custom_js: &snippets.custom_js,
                    external_font_hosts: &snippets.external_font_hosts,
                };
                match tpl.render() {
                    Ok(s) => Html(s).into_response(),
                    Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "template error").into_response(),
                }
            }),
        )
        // static files: /ui/static/{*path}
        .route("/ui/static/{*path}", get(static_handler))
        // minimal SSR pages using API + token cookie
        .route("/feeds", get(pages_feeds::ui_feeds))
        .route("/smart-views/new", get(ui_smart_view_new))
        .route("/feeds/{id}", get(pages_entries::ui_feed_entries))
        .route("/feeds/{id}/edit", get(ui_feed_edit))
        .route("/entries/{id}", get(pages_entries::ui_entry))
        .route(
            "/smart-views/{id}",
            get(pages_entries::ui_smart_view_entries),
        )
        .route("/ui/smart-views/{id}/rename", post(ui_smart_view_rename))
        .route("/ui/smart-views/create", post(ui_smart_view_create))
        .route("/ui/smart-views/{id}/update", post(ui_smart_view_update))
        .route("/ui/smart-views/{id}/delete", post(ui_smart_view_delete))
        // SSR action endpoints
        .route("/ui/entries/{id}/toggle-star", post(ui_toggle_star))
        .route("/ui/entries/{id}/mark", post(ui_mark_status))
        .route("/ui/entries/bulk-mark", post(ui_bulk_mark))
        .route("/ui/feeds/{id}/mark-all-read", post(ui_feed_mark_all_read))
        .route("/ui/feeds/{id}/refresh", post(ui_feed_refresh))
        .route("/ui/feeds/{id}/update", post(ui_feed_update))
        .route("/ui/feeds/{id}/delete", post(ui_feed_delete))
        .route("/ui/feeds/create", post(ui_feed_create))
        // settings actions
        .route("/ui/opml/export", get(pages_settings::ui_opml_export))
        .route("/ui/opml/import", post(pages_settings::ui_opml_import))
        .route(
            "/ui/api-keys/create",
            post(pages_settings::ui_apikey_create),
        )
        .route(
            "/ui/api-keys/{id}/delete",
            post(pages_settings::ui_apikey_delete),
        )
        .route(
            "/ui/integrations/create",
            post(pages_settings::ui_integration_create),
        )
        .route(
            "/ui/integrations/{id}/update",
            post(pages_settings::ui_integration_update),
        )
        .route(
            "/ui/integrations/{id}/delete",
            post(pages_settings::ui_integration_delete),
        )
        .route(
            "/ui/webhooks/create",
            post(pages_settings::ui_webhook_create),
        )
        .route(
            "/ui/webhooks/{id}/delete",
            post(pages_settings::ui_webhook_delete),
        )
        .route(
            "/ui/prefs/language",
            post(pages_settings::ui_prefs_language),
        )
        .route(
            "/ui/prefs/default-filter",
            post(pages_settings::ui_prefs_default_filter),
        )
        .route(
            "/ui/prefs/entries-per-page",
            post(pages_settings::ui_prefs_entries_per_page),
        )
        .route(
            "/ui/prefs/sort-direction",
            post(pages_settings::ui_prefs_sort_direction),
        )
        .route(
            "/ui/prefs/keyboard-shortcuts",
            post(pages_settings::ui_prefs_keyboard_shortcuts),
        )
        .route(
            "/ui/prefs/show-reading-time",
            post(pages_settings::ui_prefs_show_reading_time),
        )
        .route(
            "/ui/prefs/open-ext-newtab",
            post(pages_settings::ui_prefs_open_ext_newtab),
        )
        .route("/ui/prefs/theme", post(pages_settings::ui_prefs_theme))
        .route(
            "/ui/prefs/compact-ui",
            post(pages_settings::ui_prefs_compact_ui),
        )
        .route(
            "/ui/prefs/minimal-ui",
            post(pages_settings::ui_prefs_minimal_ui),
        )
        .route(
            "/ui/prefs/auto-mark-read",
            post(pages_settings::ui_prefs_auto_mark_read),
        )
        .route(
            "/ui/prefs/custom-css",
            post(pages_settings::ui_prefs_custom_css),
        )
        .route(
            "/ui/prefs/custom-js",
            post(pages_settings::ui_prefs_custom_js),
        )
        // categories management
        .route("/ui/categories/create", post(ui_category_create))
        .route("/ui/categories/{id}/update", post(ui_category_update))
        .route("/ui/categories/{id}/delete", post(ui_category_delete))
}

// hub pages moved to pages_hub.rs
// feeds listing moved to pages_feeds.rs

#[derive(Template)]
#[template(path = "smart_view_new.html")]
struct SmartViewNewPage<'a> {
    title: &'a str,
    dict: &'a std::collections::HashMap<String, String>,
    csp_nonce: &'a str,
    custom_css: &'a str,
    custom_js: &'a str,
    external_font_hosts: &'a str,
}

async fn ui_smart_view_new(headers: HeaderMap) -> impl IntoResponse {
    let Some(_token) = read_token_cookie(&headers) else {
        return Redirect::to("/login").into_response();
    };
    let lang = resolve_lang(&headers).await;
    let dict = i18n::load(&lang);
    let snippets = load_snippets(&headers).await;
    let nonce = gen_csp_nonce();
    let tpl = SmartViewNewPage {
        title: "New Smart View",
        dict: &dict,
        csp_nonce: &nonce,
        custom_css: &snippets.custom_css,
        custom_js: &snippets.custom_js,
        external_font_hosts: &snippets.external_font_hosts,
    };
    match tpl.render() {
        Ok(s) => Html(s).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "template error").into_response(),
    }
}

async fn ui_smart_view_rename(
    Path(id): Path<i64>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let Some(token) = read_token_cookie(&headers) else {
        return Redirect::to("/login").into_response();
    };
    let mut name: Option<String> = None;
    for (k, v) in url::form_urlencoded::parse(&body) {
        if k == "name" {
            name = Some(v.to_string());
        }
    }
    if let Some(n) = name {
        if !n.trim().is_empty() {
            let cli = match reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(4))
                .build()
            {
                Ok(c) => c,
                Err(_) => {
                    return Redirect::to(&format!("/smart-views/{}", id)).into_response();
                }
            };
            let url = format!("{}/api/v1/smart-views/{}", api_base(), id);
            let _ = cli
                .put(url)
                .header(
                    axum::http::header::AUTHORIZATION,
                    format!("Bearer {}", token),
                )
                .json(&serde_json::json!({ "name": n }))
                .send()
                .await;
        }
    }
    Redirect::to(&format!("/smart-views/{}", id)).into_response()
}

async fn ui_smart_view_create(headers: HeaderMap, body: Bytes) -> impl IntoResponse {
    let Some(token) = read_token_cookie(&headers) else {
        return Redirect::to("/login").into_response();
    };
    let mut name: Option<String> = None;
    let mut view: Option<String> = None;
    let mut status: Option<String> = None;
    let mut search: Option<String> = None;
    for (k, v) in url::form_urlencoded::parse(&body) {
        match &*k {
            "name" => {
                let s = v.to_string();
                if !s.trim().is_empty() {
                    name = Some(s);
                }
            }
            "view" => {
                let s = v.to_string();
                if !s.trim().is_empty() {
                    view = Some(s);
                }
            }
            "status" => {
                let s = v.to_string();
                if !s.trim().is_empty() {
                    status = Some(s);
                }
            }
            "search" => {
                let s = v.to_string();
                if !s.trim().is_empty() {
                    search = Some(s);
                }
            }
            _ => {}
        }
    }
    let Some(name) = name else {
        return Redirect::to("/feeds").into_response();
    };
    let view = view.unwrap_or_else(|| "all".to_string());
    let mut filters = serde_json::Map::new();
    if let Some(st) = status {
        if st != "all" {
            filters.insert("status".into(), serde_json::json!(st));
        }
    }
    if let Some(q) = search {
        filters.insert("search".into(), serde_json::json!(q));
    }
    let mut payload = serde_json::Map::new();
    payload.insert("name".into(), serde_json::json!(name));
    payload.insert("view".into(), serde_json::json!(view));
    if !filters.is_empty() {
        payload.insert("filters".into(), serde_json::Value::Object(filters));
    }
    let cli = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(4))
        .build()
    {
        Ok(c) => c,
        Err(_) => {
            return Redirect::to("/feeds").into_response();
        }
    };
    let url = format!("{}/api/v1/smart-views", api_base());
    let resp = cli
        .post(url)
        .header(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {}", token),
        )
        .json(&serde_json::Value::Object(payload))
        .send()
        .await;
    if let Ok(r) = resp.and_then(|r| r.error_for_status()) {
        if let Ok(v) = r.json::<serde_json::Value>().await {
            if let Some(id) = v.get("id").and_then(|x| x.as_i64()) {
                return Redirect::to(&format!("/smart-views/{}", id)).into_response();
            }
        }
    }
    Redirect::to("/feeds").into_response()
}

async fn ui_smart_view_update(
    Path(id): Path<i64>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let Some(token) = read_token_cookie(&headers) else {
        return Redirect::to("/login").into_response();
    };
    let mut name: Option<String> = None;
    let mut view: Option<String> = None;
    let mut status: Option<String> = None;
    let mut search: Option<String> = None;
    for (k, v) in url::form_urlencoded::parse(&body) {
        match &*k {
            "name" => {
                let s = v.to_string();
                if !s.trim().is_empty() {
                    name = Some(s);
                }
            }
            "view" => {
                let s = v.to_string();
                if !s.trim().is_empty() {
                    view = Some(s);
                }
            }
            "status" => {
                let s = v.to_string();
                if !s.trim().is_empty() {
                    status = Some(s);
                }
            }
            "search" => {
                let s = v.to_string();
                if !s.trim().is_empty() {
                    search = Some(s);
                }
            }
            _ => {}
        }
    }
    // Build partial update payload
    let mut payload = serde_json::Map::new();
    if let Some(n) = name {
        payload.insert("name".into(), serde_json::json!(n));
    }
    if let Some(v) = view {
        payload.insert("view".into(), serde_json::json!(v));
    }
    let mut filters = serde_json::Map::new();
    if let Some(st) = status {
        if st != "all" {
            filters.insert("status".into(), serde_json::json!(st));
        }
    }
    if let Some(q) = search {
        filters.insert("search".into(), serde_json::json!(q));
    }
    if !filters.is_empty() {
        payload.insert("filters".into(), serde_json::Value::Object(filters));
    }
    if !payload.is_empty() {
        let cli = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(4))
            .build()
        {
            Ok(c) => c,
            Err(_) => {
                return Redirect::to(&format!("/smart-views/{}", id)).into_response();
            }
        };
        let url = format!("{}/api/v1/smart-views/{}", api_base(), id);
        let _ = cli
            .put(url)
            .header(
                axum::http::header::AUTHORIZATION,
                format!("Bearer {}", token),
            )
            .json(&serde_json::Value::Object(payload))
            .send()
            .await;
    }
    Redirect::to(&format!("/smart-views/{}", id)).into_response()
}

async fn ui_smart_view_delete(Path(id): Path<i64>, headers: HeaderMap) -> impl IntoResponse {
    let Some(token) = read_token_cookie(&headers) else {
        return Redirect::to("/login").into_response();
    };
    let cli = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(4))
        .build()
    {
        Ok(c) => c,
        Err(_) => return Redirect::to("/feeds").into_response(),
    };
    let url = format!("{}/api/v1/smart-views/{}", api_base(), id);
    let _ = cli
        .delete(url)
        .header(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {}", token),
        )
        .send()
        .await;
    Redirect::to("/feeds").into_response()
}

// ---- SSR action handlers ----

async fn ui_toggle_star(Path(id): Path<i64>, headers: HeaderMap) -> impl IntoResponse {
    let Some(token) = read_token_cookie(&headers) else {
        return Redirect::to("/login");
    };
    let cli = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
    {
        Ok(c) => c,
        Err(_) => return Redirect::to(&format!("/entries/{}", id)),
    };
    let url = format!("{}/v1/entries/{}/star", api_base(), id);
    let _ = cli.put(url).header("X-Auth-Token", token).send().await;
    // redirect back to Referer or entry page
    let back = headers
        .get(axum::http::header::REFERER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    if back.is_empty() {
        Redirect::to(&format!("/entries/{}", id))
    } else {
        Redirect::to(&back)
    }
}

async fn ui_mark_status(Path(id): Path<i64>, headers: HeaderMap) -> impl IntoResponse {
    let Some(token) = read_token_cookie(&headers) else {
        return Redirect::to("/login");
    };
    // parse small body form status=read|unread (we accept querystring too)
    // For simplicity, read from referer query param "status=" as fallback
    // If missing, toggle read->unread
    let desired = {
        // try header x-status (not standard) then default to read
        headers
            .get("x-status")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "read".to_string())
    };
    let cli = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .unwrap();
    let url = format!("{}/v1/entries/{}", api_base(), id);
    let _ = cli
        .put(url)
        .header("X-Auth-Token", token)
        .json(&serde_json::json!({"status": desired}))
        .send()
        .await;
    let back = headers
        .get(axum::http::header::REFERER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    if back.is_empty() {
        Redirect::to(&format!("/entries/{}", id))
    } else {
        Redirect::to(&back)
    }
}

async fn ui_bulk_mark(headers: HeaderMap, body: Bytes) -> impl IntoResponse {
    let Some(token) = read_token_cookie(&headers) else {
        return Redirect::to("/login");
    };
    let parsed = url::form_urlencoded::parse(&body);
    let mut ids_str = String::new();
    let mut status = String::from("read");
    for (k, v) in parsed {
        if k == "ids" {
            ids_str = v.to_string();
        }
        if k == "status" {
            status = v.to_string();
        }
    }
    let ids: Vec<i64> = ids_str
        .split(',')
        .filter_map(|s| s.trim().parse::<i64>().ok())
        .collect();
    if !ids.is_empty() {
        let cli = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap();
        let url = format!("{}/v1/entries", api_base());
        let _ = cli
            .put(url)
            .header("X-Auth-Token", token)
            .json(&serde_json::json!({"entry_ids": ids, "status": status}))
            .send()
            .await;
    }
    let back = headers
        .get(axum::http::header::REFERER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("/feeds")
        .to_string();
    Redirect::to(&back)
}

async fn ui_feed_mark_all_read(Path(id): Path<i64>, headers: HeaderMap) -> impl IntoResponse {
    let Some(token) = read_token_cookie(&headers) else {
        return Redirect::to("/login");
    };
    let cli = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    let url = format!("{}/v1/feeds/{}/mark-all-as-read", api_base(), id);
    let _ = cli.put(url).header("X-Auth-Token", token).send().await;
    let back = headers
        .get(axum::http::header::REFERER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or(&format!("/feeds/{}", id))
        .to_string();
    Redirect::to(&back)
}

async fn ui_feed_refresh(Path(id): Path<i64>, headers: HeaderMap) -> impl IntoResponse {
    let Some(token) = read_token_cookie(&headers) else {
        return Redirect::to("/login");
    };
    let cli = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap();
    let url = format!("{}/v1/feeds/{}/refresh", api_base(), id);
    let ok = match cli.put(url).header("X-Auth-Token", token).send().await {
        Ok(r) => r.status().is_success(),
        Err(_) => false,
    };
    let mut back = headers
        .get(axum::http::header::REFERER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or(&format!("/feeds/{}", id))
        .to_string();
    if back.contains('?') {
        back.push('&');
    } else {
        back.push('?');
    }
    back.push_str(if ok { "refreshed=1" } else { "refresh_err=1" });
    Redirect::to(&back)
}

async fn ui_feed_create(headers: HeaderMap, body: Bytes) -> impl IntoResponse {
    let Some(token) = read_token_cookie(&headers) else {
        return Redirect::to("/login").into_response();
    };
    let mut url = String::new();
    let mut title = None::<String>;
    let mut category_id = None::<i64>;
    for (k, v) in url::form_urlencoded::parse(&body) {
        match &*k {
            "feed_url" => url = v.to_string(),
            "title" => {
                let s = v.to_string();
                if !s.trim().is_empty() {
                    title = Some(s);
                }
            }
            "category_id" => {
                if let Ok(n) = v.parse::<i64>() {
                    if n > 0 {
                        category_id = Some(n);
                    }
                }
            }
            _ => {}
        }
    }
    if url.trim().is_empty() {
        return Redirect::to("/feeds").into_response();
    }
    let payload = serde_json::json!({ "url": url, "title": title, "category_id": category_id });
    let cli = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap();
    let api = format!("{}/v1/feeds", api_base());
    let resp = cli
        .post(api)
        .header("X-Auth-Token", token)
        .json(&payload)
        .send()
        .await;
    if let Ok(r) = resp.and_then(|r| r.error_for_status()) {
        if let Ok(v) = r.json::<serde_json::Value>().await {
            if let Some(id) = v.get("id").and_then(|x| x.as_i64()) {
                return Redirect::to(&format!("/feeds/{}?status=unread", id)).into_response();
            }
        }
    }
    Redirect::to("/feeds").into_response()
}

#[derive(Deserialize, Clone)]
struct UiFeedFull {
    id: i64,
    title: Option<String>,
    #[serde(default)]
    category: Option<pages_feeds::UiCategory>,
    #[serde(default)]
    user_agent: Option<String>,
    #[serde(default)]
    proxy_url: Option<String>,
    #[serde(default)]
    fetch_via_proxy: Option<bool>,
    #[serde(default)]
    disable_http2: Option<bool>,
    #[serde(default, rename = "allow_self_signed_certificates")]
    allow_invalid_certs: Option<bool>,
    #[serde(default)]
    request_timeout_ms: Option<i32>,
    #[serde(default)]
    cookie: Option<String>,
    #[serde(default)]
    scraper_rules: Option<String>,
    #[serde(default)]
    rewrite_rules: Option<String>,
    #[serde(default, rename = "urlrewrite_rules")]
    url_rewrite_rules: Option<String>,
    #[serde(default)]
    blocklist_rules: Option<String>,
    #[serde(default)]
    keeplist_rules: Option<String>,
}

#[derive(askama::Template)]
#[template(path = "feed_edit.html")]
struct FeedEditPage<'a> {
    title: &'a str,
    feed: &'a UiFeedFull,
    categories: &'a [pages_feeds::UiCategory],
    dict: &'a std::collections::HashMap<String, String>,
    csp_nonce: &'a str,
    custom_css: &'a str,
    custom_js: &'a str,
    external_font_hosts: &'a str,
}

async fn ui_feed_edit(Path(id): Path<i64>, headers: HeaderMap) -> impl IntoResponse {
    let Some(token) = read_token_cookie(&headers) else {
        return Redirect::to("/login").into_response();
    };
    let lang = resolve_lang(&headers).await;
    let dict = i18n::load(&lang);
    let snippets = load_snippets(&headers).await;
    let nonce = gen_csp_nonce();
    let cli = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(4))
        .build()
        .unwrap();
    let url = format!("{}/v1/feeds/{}", api_base(), id);
    let feed: UiFeedFull = match cli
        .get(&url)
        .header("X-Auth-Token", &token)
        .send()
        .await
        .and_then(|r| r.error_for_status())
    {
        Ok(resp) => resp.json().await.unwrap_or(UiFeedFull {
            id,
            title: None,
            category: None,
            user_agent: None,
            proxy_url: None,
            fetch_via_proxy: None,
            disable_http2: None,
            allow_invalid_certs: None,
            request_timeout_ms: None,
            cookie: None,
            scraper_rules: None,
            rewrite_rules: None,
            url_rewrite_rules: None,
            blocklist_rules: None,
            keeplist_rules: None,
        }),
        Err(_) => UiFeedFull {
            id,
            title: None,
            category: None,
            user_agent: None,
            proxy_url: None,
            fetch_via_proxy: None,
            disable_http2: None,
            allow_invalid_certs: None,
            request_timeout_ms: None,
            cookie: None,
            scraper_rules: None,
            rewrite_rules: None,
            url_rewrite_rules: None,
            blocklist_rules: None,
            keeplist_rules: None,
        },
    };
    let cats_url = format!("{}/v1/categories?counts=false", api_base());
    let categories: Vec<pages_feeds::UiCategory> = match cli
        .get(cats_url)
        .header("X-Auth-Token", token)
        .send()
        .await
        .and_then(|r| r.error_for_status())
    {
        Ok(resp) => resp.json().await.unwrap_or_default(),
        Err(_) => vec![],
    };
    let tpl = FeedEditPage {
        title: "Edit Feed",
        feed: &feed,
        categories: &categories,
        dict: &dict,
        csp_nonce: &nonce,
        custom_css: &snippets.custom_css,
        custom_js: &snippets.custom_js,
        external_font_hosts: &snippets.external_font_hosts,
    };
    match tpl.render() {
        Ok(s) => Html(s).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "template error").into_response(),
    }
}

async fn ui_feed_update(Path(id): Path<i64>, headers: HeaderMap, body: Bytes) -> impl IntoResponse {
    let Some(token) = read_token_cookie(&headers) else {
        return Redirect::to("/login").into_response();
    };
    let mut title: Option<String> = None;
    let mut category_id: Option<i64> = None;
    let mut user_agent: Option<String> = None;
    let mut proxy_url: Option<String> = None;
    let mut cookie: Option<String> = None;
    let mut fetch_via_proxy: Option<bool> = None;
    let mut disable_http2: Option<bool> = None;
    let mut allow_invalid_certs: Option<bool> = None;
    let mut request_timeout_ms: Option<i32> = None;
    let mut scraper_rules: Option<String> = None;
    let mut rewrite_rules: Option<String> = None;
    let mut url_rewrite_rules: Option<String> = None;
    let mut blocklist_rules: Option<String> = None;
    let mut keeplist_rules: Option<String> = None;
    for (k, v) in url::form_urlencoded::parse(&body) {
        match &*k {
            "title" => {
                let s = v.to_string();
                if !s.trim().is_empty() {
                    title = Some(s);
                } else {
                    title = Some(String::new());
                }
            }
            "category_id" => {
                let s = v.to_string();
                if let Ok(n) = s.parse::<i64>() {
                    category_id = Some(n);
                } else {
                    category_id = None;
                }
            }
            "user_agent" => {
                let s = v.to_string();
                if !s.trim().is_empty() {
                    user_agent = Some(s);
                } else {
                    user_agent = None;
                }
            }
            "proxy_url" => {
                let s = v.to_string();
                if !s.trim().is_empty() {
                    proxy_url = Some(s);
                } else {
                    proxy_url = None;
                }
            }
            "cookie" => {
                let s = v.to_string();
                if !s.trim().is_empty() {
                    cookie = Some(s);
                } else {
                    cookie = None;
                }
            }
            "fetch_via_proxy" => {
                fetch_via_proxy = Some(v == "on" || v == "1" || v.eq_ignore_ascii_case("true"));
            }
            "disable_http2" => {
                disable_http2 = Some(v == "on" || v == "1" || v.eq_ignore_ascii_case("true"));
            }
            "allow_invalid_certs" => {
                allow_invalid_certs = Some(v == "on" || v == "1" || v.eq_ignore_ascii_case("true"));
            }
            "request_timeout_ms" => {
                if let Ok(n) = v.to_string().parse::<i32>() {
                    request_timeout_ms = Some(n);
                }
            }
            "scraper_rules" => {
                let s = v.to_string();
                scraper_rules = Some(s);
            }
            "rewrite_rules" => {
                let s = v.to_string();
                rewrite_rules = Some(s);
            }
            "url_rewrite_rules" => {
                let s = v.to_string();
                url_rewrite_rules = Some(s);
            }
            "blocklist_rules" => {
                let s = v.to_string();
                blocklist_rules = Some(s);
            }
            "keeplist_rules" => {
                let s = v.to_string();
                keeplist_rules = Some(s);
            }
            _ => {}
        }
    }
    let mut payload = serde_json::Map::new();
    if let Some(t) = title {
        payload.insert("title".into(), serde_json::Value::String(t));
    }
    // Allow null to remove the category assignment
    payload.insert(
        "category_id".into(),
        match category_id {
            Some(n) => serde_json::Value::Number(n.into()),
            None => serde_json::Value::Null,
        },
    );
    if let Some(s) = user_agent {
        payload.insert("user_agent".into(), serde_json::Value::String(s));
    }
    if let Some(s) = proxy_url {
        payload.insert("proxy_url".into(), serde_json::Value::String(s));
    }
    if let Some(s) = cookie {
        payload.insert("cookie".into(), serde_json::Value::String(s));
    }
    if let Some(b) = fetch_via_proxy {
        payload.insert("fetch_via_proxy".into(), serde_json::Value::Bool(b));
    }
    if let Some(b) = disable_http2 {
        payload.insert("disable_http2".into(), serde_json::Value::Bool(b));
    }
    if let Some(b) = allow_invalid_certs {
        payload.insert(
            "allow_self_signed_certificates".into(),
            serde_json::Value::Bool(b),
        );
    }
    if let Some(n) = request_timeout_ms {
        payload.insert(
            "request_timeout_ms".into(),
            serde_json::Value::Number((n as i64).into()),
        );
    }
    if let Some(s) = scraper_rules {
        payload.insert("scraper_rules".into(), serde_json::Value::String(s));
    }
    if let Some(s) = rewrite_rules {
        payload.insert("rewrite_rules".into(), serde_json::Value::String(s));
    }
    if let Some(s) = url_rewrite_rules {
        payload.insert("urlrewrite_rules".into(), serde_json::Value::String(s));
    }
    if let Some(s) = blocklist_rules {
        payload.insert("blocklist_rules".into(), serde_json::Value::String(s));
    }
    if let Some(s) = keeplist_rules {
        payload.insert("keeplist_rules".into(), serde_json::Value::String(s));
    }
    let cli = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    let url = format!("{}/v1/feeds/{}", api_base(), id);
    let _ = cli
        .put(url)
        .header("X-Auth-Token", token)
        .json(&payload)
        .send()
        .await;
    Redirect::to("/feeds").into_response()
}

async fn ui_feed_delete(Path(id): Path<i64>, headers: HeaderMap) -> impl IntoResponse {
    let Some(token) = read_token_cookie(&headers) else {
        return Redirect::to("/login").into_response();
    };
    let cli = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    let url = format!("{}/v1/feeds/{}", api_base(), id);
    let _ = cli.delete(url).header("X-Auth-Token", token).send().await;
    Redirect::to("/feeds").into_response()
}

async fn ui_category_create(headers: HeaderMap, body: Bytes) -> impl IntoResponse {
    let Some(token) = read_token_cookie(&headers) else {
        return Redirect::to("/login").into_response();
    };
    let mut title = String::new();
    for (k, v) in url::form_urlencoded::parse(&body) {
        if k == "title" {
            title = v.to_string();
        }
    }
    if !title.trim().is_empty() {
        let cli = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap();
        let url = format!("{}/v1/categories", api_base());
        let _ = cli
            .post(url)
            .header("X-Auth-Token", token)
            .json(&serde_json::json!({"title": title}))
            .send()
            .await;
    }
    Redirect::to("/feeds").into_response()
}

async fn ui_category_update(
    Path(id): Path<i64>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let Some(token) = read_token_cookie(&headers) else {
        return Redirect::to("/login").into_response();
    };
    let mut title = None;
    for (k, v) in url::form_urlencoded::parse(&body) {
        if k == "title" {
            title = Some(v.to_string());
        }
    }
    if let Some(t) = title {
        if !t.trim().is_empty() {
            let cli = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .unwrap();
            let url = format!("{}/v1/categories/{}", api_base(), id);
            let _ = cli
                .put(url)
                .header("X-Auth-Token", token)
                .json(&serde_json::json!({"title": t}))
                .send()
                .await;
        }
    }
    Redirect::to("/feeds").into_response()
}

async fn ui_category_delete(Path(id): Path<i64>, headers: HeaderMap) -> impl IntoResponse {
    let Some(token) = read_token_cookie(&headers) else {
        return Redirect::to("/login").into_response();
    };
    let cli = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    let url = format!("{}/v1/categories/{}", api_base(), id);
    let _ = cli.delete(url).header("X-Auth-Token", token).send().await;
    Redirect::to("/feeds").into_response()
}
