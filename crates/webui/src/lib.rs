//! Minimal SSR Web UI for Captura (Askama-based).
//! This crate exposes a Router that can be mounted by the API service.
//! It deliberately avoids frontend frameworks and large inline scripts/styles
//! and keeps CSP strict by using nonces for small user-provided CSS/JS.

use askama::Template;
use axum::{
    body::Bytes,
    extract::{Path, Query},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
    Router,
};
use rand_core::RngCore;
use serde::Deserialize;
use std::time::Duration;

mod static_assets;
use static_assets::static_handler;
mod i18n;

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

fn gen_csp_nonce() -> String {
    use base64::Engine as _;
    let mut buf = [0u8; 16];
    rand_core::OsRng.fill_bytes(&mut buf);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf)
}

#[derive(Default)]
struct UiSnippets {
    custom_css: String,
    custom_js: String,
    external_font_hosts: String,
}

async fn load_snippets(headers: &HeaderMap) -> UiSnippets {
    let Some(token) = cookie_value(headers, "X-Auth-Token") else {
        return UiSnippets::default();
    };
    let cli = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    let me = format!("{}/v1/me", api_base());
    if let Ok(resp) = cli
        .get(me)
        .header("X-Auth-Token", token)
        .send()
        .await
        .and_then(|r| r.error_for_status())
    {
        #[derive(serde::Deserialize)]
        struct Me {
            stylesheet: Option<String>,
            custom_js: Option<String>,
            external_font_hosts: Option<String>,
        }
        if let Ok(m) = resp.json::<Me>().await {
            return UiSnippets {
                custom_css: m.stylesheet.unwrap_or_default(),
                custom_js: m.custom_js.unwrap_or_default(),
                external_font_hosts: m.external_font_hosts.unwrap_or_default(),
            };
        }
    }
    UiSnippets::default()
}

#[derive(Template)]
#[allow(dead_code)]
#[template(path = "layout.html")]
struct LayoutTemplate<'a> {
    title: &'a str,
    body_html: &'a str,
    dict: &'a std::collections::HashMap<String, String>,
    csp_nonce: &'a str,
    custom_css: &'a str,
    custom_js: &'a str,
    external_font_hosts: &'a str,
}

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate<'a> {
    title: &'a str,
    dict: &'a std::collections::HashMap<String, String>,
    csp_nonce: &'a str,
    custom_css: &'a str,
    custom_js: &'a str,
    external_font_hosts: &'a str,
}

#[derive(Template)]
#[template(path = "hub_routes.html")]
struct HubRoutesPage<'a> {
    title: &'a str,
    routes: &'a [UiHubRoute],
    preview: Option<UiHubPreview>,
    preview_url: &'a str,
    dict: &'a std::collections::HashMap<String, String>,
    csp_nonce: &'a str,
    custom_css: &'a str,
    custom_js: &'a str,
    external_font_hosts: &'a str,
}

#[derive(Template)]
#[template(path = "login.html")]
struct LoginTemplate<'a> {
    title: &'a str,
    oidc_enabled: bool,
    dict: &'a std::collections::HashMap<String, String>,
    csp_nonce: &'a str,
    custom_css: &'a str,
    custom_js: &'a str,
    external_font_hosts: &'a str,
}

#[derive(Template)]
#[template(path = "signup.html")]
struct SignupTemplate<'a> {
    title: &'a str,
    dict: &'a std::collections::HashMap<String, String>,
    csp_nonce: &'a str,
    custom_css: &'a str,
    custom_js: &'a str,
    external_font_hosts: &'a str,
}

async fn index(headers: HeaderMap) -> axum::response::Response {
    let lang = resolve_lang(&headers).await;
    let dict = i18n::load(&lang);
    let nonce = gen_csp_nonce();
    let tpl = IndexTemplate {
        title: "Captura",
        dict: &dict,
        csp_nonce: &nonce,
        custom_css: "",
        custom_js: "",
        external_font_hosts: "",
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

async fn login(headers: HeaderMap) -> axum::response::Response {
    // The page points to existing API endpoints. This handler does not depend on server state.
    let lang = resolve_lang(&headers).await;
    let dict = i18n::load(&lang);
    let enabled = std::env::var("CAPTURA_OIDC_ENABLED")
        .ok()
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let nonce = gen_csp_nonce();
    let tpl = LoginTemplate {
        title: "Login",
        oidc_enabled: enabled,
        dict: &dict,
        csp_nonce: &nonce,
        custom_css: "",
        custom_js: "",
        external_font_hosts: "",
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

async fn signup(headers: HeaderMap) -> axum::response::Response {
    let lang = resolve_lang(&headers).await;
    let dict = i18n::load(&lang);
    let nonce = gen_csp_nonce();
    let tpl = SignupTemplate {
        title: "Sign Up",
        dict: &dict,
        csp_nonce: &nonce,
        custom_css: "",
        custom_js: "",
        external_font_hosts: "",
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

/// Build a UI router with generic state.
/// No handler here takes a dependency on the state so S can be any shared state.
pub fn router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/", get(index))
        .route("/login", get(login))
        .route("/signup", get(signup))
        .route("/settings", get(ui_settings))
        .route("/hub", get(ui_hub_routes))
        // static files: /ui/static/{*path}
        .route("/ui/static/{*path}", get(static_handler))
        // minimal SSR pages using API + token cookie
        .route("/feeds", get(ui_feeds))
        .route("/feeds/{id}", get(ui_feed_entries))
        .route("/feeds/{id}/edit", get(ui_feed_edit))
        .route("/entries/{id}", get(ui_entry))
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
        .route("/ui/opml/export", get(ui_opml_export))
        .route("/ui/opml/import", post(ui_opml_import))
        .route("/ui/api-keys/create", post(ui_apikey_create))
        .route("/ui/api-keys/{id}/delete", post(ui_apikey_delete))
        .route("/ui/integrations/create", post(ui_integration_create))
        .route("/ui/integrations/{id}/update", post(ui_integration_update))
        .route("/ui/integrations/{id}/delete", post(ui_integration_delete))
        .route("/ui/webhooks/create", post(ui_webhook_create))
        .route("/ui/webhooks/{id}/delete", post(ui_webhook_delete))
        .route("/ui/prefs/language", post(ui_prefs_language))
        .route("/ui/prefs/default-filter", post(ui_prefs_default_filter))
        .route(
            "/ui/prefs/entries-per-page",
            post(ui_prefs_entries_per_page),
        )
        .route("/ui/prefs/sort-direction", post(ui_prefs_sort_direction))
        .route(
            "/ui/prefs/keyboard-shortcuts",
            post(ui_prefs_keyboard_shortcuts),
        )
        .route(
            "/ui/prefs/show-reading-time",
            post(ui_prefs_show_reading_time),
        )
        .route("/ui/prefs/open-ext-newtab", post(ui_prefs_open_ext_newtab))
        .route("/ui/prefs/theme", post(ui_prefs_theme))
        .route("/ui/prefs/compact-ui", post(ui_prefs_compact_ui))
        .route("/ui/prefs/minimal-ui", post(ui_prefs_minimal_ui))
        .route("/ui/prefs/auto-mark-read", post(ui_prefs_auto_mark_read))
        .route("/ui/prefs/custom-css", post(ui_prefs_custom_css))
        .route("/ui/prefs/custom-js", post(ui_prefs_custom_js))
        // categories management
        .route("/ui/categories/create", post(ui_category_create))
        .route("/ui/categories/{id}/update", post(ui_category_update))
        .route("/ui/categories/{id}/delete", post(ui_category_delete))
}

// -------------- templates (embedded) --------------
// We place templates under `templates/` co-located with crate.

// askama looks up templates under a `templates` dir sitting next to Cargo.toml

// -------------- static assets --------------

fn read_token_cookie(headers: &HeaderMap) -> Option<String> {
    let cookies = headers.get(axum::http::header::COOKIE)?.to_str().ok()?;
    for part in cookies.split(';') {
        let kv = part.trim();
        if let Some((k, v)) = kv.split_once('=') {
            if k.trim() == "X-Auth-Token" {
                return Some(v.trim().to_string());
            }
        }
    }
    None
}

fn api_base() -> String {
    std::env::var("CAPTURA_WEBUI_API_BASE")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "http://127.0.0.1:8080".into())
}

fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    let cookies = headers.get(axum::http::header::COOKIE)?.to_str().ok()?;
    for part in cookies.split(';') {
        let kv = part.trim();
        if let Some((k, v)) = kv.split_once('=') {
            if k.trim() == name {
                return Some(v.trim().to_string());
            }
        }
    }
    None
}

async fn resolve_lang(headers: &HeaderMap) -> String {
    if let Some(lang) = cookie_value(headers, "lang") {
        return lang;
    }
    if let Some(token) = cookie_value(headers, "X-Auth-Token") {
        if let Ok(cli) = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(2))
            .build()
        {
            let me = format!("{}/v1/me", api_base());
            if let Ok(resp) = cli
                .get(me)
                .header("X-Auth-Token", token)
                .send()
                .await
                .and_then(|r| r.error_for_status())
            {
                #[derive(serde::Deserialize)]
                struct Me {
                    language: Option<String>,
                }
                if let Ok(m) = resp.json::<Me>().await {
                    if let Some(l) = m.language {
                        return l;
                    }
                }
            }
        }
    }
    // Accept-Language heuristic
    if let Some(al) = headers
        .get(axum::http::header::ACCEPT_LANGUAGE)
        .and_then(|v| v.to_str().ok())
    {
        let al = al.to_ascii_lowercase();
        if al.contains("zh") {
            return "zh_CN".into();
        }
    }
    "en_US".into()
}

#[allow(dead_code)]
#[derive(Deserialize, Clone)]
struct UiHubRoute {
    hub_id: String,
    path: String,
    categories: Vec<String>,
    example: String,
    #[serde(default)]
    parameters: Vec<(String, String)>,
    name: String,
    url: String,
    description: String,
}

#[allow(dead_code)]
#[derive(Deserialize, Clone)]
struct UiHubItem {
    title: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    link: Option<String>,
    #[serde(default)]
    author: Option<String>,
}

#[allow(dead_code)]
#[derive(Deserialize, Clone)]
struct UiHubPreview {
    title: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    link: Option<String>,
    #[serde(default)]
    image: Option<String>,
    #[serde(default)]
    language: Option<String>,
    items: Vec<UiHubItem>,
}

#[derive(Deserialize, Default)]
struct UiHubQuery {
    url: Option<String>,
}

async fn ui_hub_routes(headers: HeaderMap, Query(q): Query<UiHubQuery>) -> impl IntoResponse {
    let Some(token) = read_token_cookie(&headers) else {
        return Redirect::to("/login").into_response();
    };
    let lang = resolve_lang(&headers).await;
    let dict = i18n::load(&lang);
    let snippets = load_snippets(&headers).await;
    let nonce = gen_csp_nonce();

    let cli = match reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
    {
        Ok(c) => c,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "http client error").into_response(),
    };

    #[derive(Deserialize)]
    struct HubRoutesResp {
        routes: Vec<UiHubRoute>,
    }

    // Fetch hub routes list.
    let routes_url = format!("{}/api/v1/hub/routes", api_base());
    let routes: Vec<UiHubRoute> = match cli
        .get(routes_url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .and_then(|r| r.error_for_status())
    {
        Ok(resp) => resp
            .json::<HubRoutesResp>()
            .await
            .map(|r| r.routes)
            .unwrap_or_default(),
        Err(_) => Vec::new(),
    };

    // Optional preview.
    let mut preview: Option<UiHubPreview> = None;
    let preview_url = q.url.unwrap_or_default();
    if !preview_url.is_empty() {
        #[derive(Deserialize)]
        struct PreviewResp {
            data: UiHubPreview,
        }
        let preview_endpoint = format!("{}/api/v1/hub/preview", api_base());
        let body = serde_json::json!({ "url": preview_url });
        if let Ok(resp) = cli
            .post(preview_endpoint)
            .header("Authorization", format!("Bearer {}", token))
            .json(&body)
            .send()
            .await
            .and_then(|r| r.error_for_status())
        {
            if let Ok(pr) = resp.json::<PreviewResp>().await {
                preview = Some(pr.data);
            }
        }
    }

    let tpl = HubRoutesPage {
        title: "Hub Routes",
        routes: &routes,
        preview,
        preview_url: &preview_url,
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

#[derive(Deserialize, Clone)]
#[allow(dead_code)]
struct UiFeedDto {
    id: i64,
    title: Option<String>,
    site_url: Option<String>,
    unread_count: Option<i64>,
    category: Option<UiCategory>,
    #[serde(default)]
    parsing_error_count: Option<i32>,
    #[serde(default)]
    parsing_error_message: Option<String>,
}

#[derive(Deserialize, Clone)]
struct UiCategory {
    id: i64,
    title: String,
    #[serde(default)]
    feed_count: Option<i64>,
    #[serde(default)]
    total_unread: Option<i64>,
}

#[derive(Template)]
#[template(path = "feeds.html")]
struct FeedsPage<'a> {
    title: &'a str,
    feeds: &'a [UiFeedDto],
    categories: &'a [UiCategory],
    selected_category: Option<i64>,
    has_uncategorized: bool,
    dict: &'a std::collections::HashMap<String, String>,
    csp_nonce: &'a str,
    custom_css: &'a str,
    custom_js: &'a str,
    external_font_hosts: &'a str,
}

#[derive(Deserialize, Default)]
struct UiFeedsQuery {
    category_id: Option<i64>,
}

async fn ui_feeds(headers: HeaderMap, Query(fq): Query<UiFeedsQuery>) -> impl IntoResponse {
    let Some(token) = read_token_cookie(&headers) else {
        return Redirect::to("/login").into_response();
    };
    let lang = resolve_lang(&headers).await;
    let dict = i18n::load(&lang);
    let snippets = load_snippets(&headers).await;
    let nonce = gen_csp_nonce();
    let cli = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
    {
        Ok(c) => c,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "http client error").into_response(),
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
    let res2 = cli.get(cats_url).header("X-Auth-Token", token).send().await;
    let categories: Vec<UiCategory> = match res2.and_then(|r| r.error_for_status()) {
        Ok(resp) => resp.json().await.unwrap_or_default(),
        Err(_) => Vec::new(),
    };
    let tpl = FeedsPage {
        title: "Feeds",
        feeds: &feeds,
        categories: &categories,
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
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "template error").into_response(),
    }
}

#[derive(Deserialize, Clone)]
#[allow(dead_code)]
struct UiEntryBrief {
    id: i64,
    title: Option<String>,
    url: Option<String>,
    author: Option<String>,
    #[serde(rename = "published_at")]
    date: Option<String>,
    starred: bool,
    status: String,
    #[serde(default)]
    tags: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct UiEntrySet {
    total: i64,
    entries: Vec<UiEntryBrief>,
}

#[derive(Template)]
#[template(path = "entries.html")]
struct EntriesPage<'a> {
    title: &'a str,
    feed_id: i64,
    items: &'a [UiEntryBrief],
    limit: usize,
    prev_page: Option<usize>,
    next_page: Option<usize>,
    dict: &'a std::collections::HashMap<String, String>,
    filter: &'a str,
    filter_q: &'a str,
    search_q_qs: &'a str,
    search_q: &'a str,
    refreshed: bool,
    refresh_err: bool,
    csp_nonce: &'a str,
    custom_css: &'a str,
    custom_js: &'a str,
    external_font_hosts: &'a str,
}

#[derive(Deserialize, Default)]
struct UiListQuery {
    page: Option<usize>,
    limit: Option<usize>,
    status: Option<String>,
    starred: Option<bool>,
    q: Option<String>,
    refreshed: Option<bool>,
    refresh_err: Option<bool>,
}

async fn ui_feed_entries(
    Path(id): Path<i64>,
    headers: HeaderMap,
    Query(q): Query<UiListQuery>,
) -> impl IntoResponse {
    let Some(token) = read_token_cookie(&headers) else {
        return Redirect::to("/login").into_response();
    };
    let lang = resolve_lang(&headers).await;
    let dict = i18n::load(&lang);
    let snippets = load_snippets(&headers).await;
    let nonce = gen_csp_nonce();
    let cli = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(4))
        .build()
    {
        Ok(c) => c,
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "http client error").into_response();
        }
    };
    // If limit not specified, read from /v1/me (entries_per_page)
    let limit = if let Some(l) = q.limit {
        l.clamp(1, 200)
    } else {
        let me_url = format!("{}/v1/me", api_base());
        match cli
            .get(me_url)
            .header("X-Auth-Token", &token)
            .send()
            .await
            .and_then(|r| r.error_for_status())
        {
            Ok(resp) => {
                #[derive(serde::Deserialize)]
                struct Me {
                    entries_per_page: Option<i32>,
                }
                let me: Me = resp.json().await.unwrap_or(Me {
                    entries_per_page: None,
                });
                me.entries_per_page.unwrap_or(50).max(1) as usize
            }
            Err(_) => 50usize,
        }
        .min(200)
    };
    let page = q.page.unwrap_or(1).max(1);
    let offset = (page - 1) * limit;
    let mut url = format!(
        "{}/v1/feeds/{}/entries?limit={}&offset={}&order=published_at&direction=desc",
        api_base(),
        id,
        limit,
        offset
    );
    let mut filter = "all".to_string();
    let mut filter_q = String::new();
    if let Some(ref s) = q.status {
        let s = s.trim().to_lowercase();
        if s == "unread" || s == "read" {
            url.push_str(&format!("&status={}", s));
            filter = s;
            filter_q = format!("&status={}", filter);
        }
    }
    if let Some(st) = q.starred {
        if st {
            url.push_str("&starred=true");
            filter = "starred".into();
            filter_q = "&starred=true".into();
        }
    }
    // search query
    let mut search_q_qs = String::new();
    let mut search_q_value = String::new();
    if let Some(ref sq) = q.q {
        if !sq.trim().is_empty() {
            let enc = urlencoding::encode(sq);
            url.push_str(&format!("&q={}", enc));
            search_q_qs = format!("&q={}", enc);
            search_q_value = sq.clone();
        }
    }
    // If user didn't pass any filter, apply cookie default_filter
    if q.status.is_none() && q.starred.is_none() {
        if let Some(def) = cookie_value(&headers, "default_filter") {
            let d = def.to_ascii_lowercase();
            if d == "unread" {
                url.push_str("&status=unread");
                filter = "unread".into();
                filter_q = "&status=unread".into();
            } else if d == "starred" {
                url.push_str("&starred=true");
                filter = "starred".into();
                filter_q = "&starred=true".into();
            }
        }
    }
    let res = cli
        .get(url)
        .header("X-Auth-Token", token.clone())
        .send()
        .await;
    let set: UiEntrySet = match res.and_then(|r| r.error_for_status()) {
        Ok(resp) => resp.json().await.unwrap_or(UiEntrySet {
            total: 0,
            entries: vec![],
        }),
        Err(_) => UiEntrySet {
            total: 0,
            entries: vec![],
        },
    };
    let total = set.total.max(0) as usize;
    let end_index = offset + set.entries.len();
    let prev_page = if page > 1 { Some(page - 1) } else { None };
    let next_page = if end_index < total {
        Some(page + 1)
    } else {
        None
    };
    let refreshed = q.refreshed.unwrap_or(false);
    let refresh_err = q.refresh_err.unwrap_or(false);
    let filter_leaked = Box::leak(filter.into_boxed_str());
    let filter_q_leaked = Box::leak(filter_q.into_boxed_str());
    let search_q_qs_leaked = Box::leak(search_q_qs.into_boxed_str());
    let search_q_leaked = Box::leak(search_q_value.into_boxed_str());
    let tpl = EntriesPage {
        title: "Entries",
        feed_id: id,
        items: &set.entries,
        limit,
        prev_page,
        next_page,
        dict: &dict,
        filter: filter_leaked,
        filter_q: filter_q_leaked,
        search_q_qs: search_q_qs_leaked,
        search_q: search_q_leaked,
        refreshed,
        refresh_err,
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

#[derive(Deserialize)]
struct UiEntryFull {
    id: i64,
    title: Option<String>,
    author: Option<String>,
    url: Option<String>,
    content: Option<String>,
    status: String,
    starred: bool,
    feed_id: i64,
    #[serde(default)]
    tags: Option<Vec<String>>,
}

#[derive(Template)]
#[template(path = "entry.html")]
struct EntryPage<'a> {
    title: &'a str,
    entry: &'a UiEntryFull,
    prev_id: Option<i64>,
    next_id: Option<i64>,
    dict: &'a std::collections::HashMap<String, String>,
    csp_nonce: &'a str,
    custom_css: &'a str,
    custom_js: &'a str,
    external_font_hosts: &'a str,
}

async fn ui_entry(Path(id): Path<i64>, headers: HeaderMap) -> impl IntoResponse {
    let Some(token) = read_token_cookie(&headers) else {
        return Redirect::to("/login").into_response();
    };
    let lang = resolve_lang(&headers).await;
    let dict = i18n::load(&lang);
    let snippets = load_snippets(&headers).await;
    let nonce = gen_csp_nonce();
    let cli = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(4))
        .build()
    {
        Ok(c) => c,
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "http client error").into_response();
        }
    };
    let url = format!("{}/v1/entries/{}", api_base(), id);
    let res = cli
        .get(url)
        .header("X-Auth-Token", token.clone())
        .send()
        .await;
    let entry: UiEntryFull = match res.and_then(|r| r.error_for_status()) {
        Ok(resp) => resp.json().await.unwrap_or(UiEntryFull {
            id,
            title: None,
            author: None,
            url: None,
            content: None,
            status: String::new(),
            starred: false,
            feed_id: 0,
            tags: None,
        }),
        Err(_) => UiEntryFull {
            id,
            title: None,
            author: None,
            url: None,
            content: None,
            status: String::new(),
            starred: false,
            feed_id: 0,
            tags: None,
        },
    };
    let (mut prev_id, mut next_id) = (None, None);
    if entry.feed_id > 0 {
        // prev: before_id current
        let prev_url = format!(
            "{}/v1/entries?feed_id={}&before_id={}&order=id&direction=desc&limit=1",
            api_base(),
            entry.feed_id,
            entry.id
        );
        if let Ok(r) = cli
            .get(prev_url)
            .header("X-Auth-Token", &token)
            .send()
            .await
            .and_then(|r| r.error_for_status())
        {
            if let Ok(s) = r.json::<UiEntrySet>().await {
                if let Some(e) = s.entries.first() {
                    prev_id = Some(e.id);
                }
            }
        }
        // next: after_id current
        let next_url = format!(
            "{}/v1/entries?feed_id={}&after_id={}&order=id&direction=asc&limit=1",
            api_base(),
            entry.feed_id,
            entry.id
        );
        if let Ok(r) = cli
            .get(next_url)
            .header("X-Auth-Token", &token)
            .send()
            .await
            .and_then(|r| r.error_for_status())
        {
            if let Ok(s) = r.json::<UiEntrySet>().await {
                if let Some(e) = s.entries.first() {
                    next_id = Some(e.id);
                }
            }
        }
    }
    let tpl = EntryPage {
        title: "Entry",
        entry: &entry,
        prev_id,
        next_id,
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

// ---------------- Settings pages ----------------

#[derive(Deserialize, Clone)]
#[allow(dead_code)]
struct UiApiKey {
    id: i64,
    user_id: i64,
    token: String,
    description: Option<String>,
    last_used_at: Option<String>,
    created_at: String,
}

#[derive(Deserialize, Clone)]
struct UiIntegration {
    id: i64,
    kind: String,
    enabled: bool,
    config_json: Option<serde_json::Value>,
}

#[derive(Deserialize, Clone)]
#[allow(dead_code)]
struct UiWebhook {
    id: i64,
    url: String,
    events: Option<String>,
    enabled: bool,
    created_at: String,
}

#[derive(Template)]
#[template(path = "settings.html")]
struct SettingsPage<'a> {
    title: &'a str,
    apikeys: &'a [UiApiKey],
    integrations: &'a [UiIntegration],
    webhooks: &'a [UiWebhook],
    dict: &'a std::collections::HashMap<String, String>,
    lang: &'a str,
    default_filter: &'a str,
    entries_per_page: i32,
    csp_nonce: &'a str,
    custom_css: &'a str,
    custom_js: &'a str,
    external_font_hosts: &'a str,
}

async fn ui_settings(headers: HeaderMap) -> impl IntoResponse {
    let Some(token) = read_token_cookie(&headers) else {
        return Redirect::to("/login").into_response();
    };
    let lang = resolve_lang(&headers).await;
    let dict = i18n::load(&lang);
    let snippets = load_snippets(&headers).await;
    let nonce = gen_csp_nonce();
    let cli = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    let keys_url = format!("{}/v1/api-keys", api_base());
    let apikeys: Vec<UiApiKey> = match cli
        .get(keys_url)
        .header("X-Auth-Token", &token)
        .send()
        .await
        .and_then(|r| r.error_for_status())
    {
        Ok(resp) => resp.json().await.unwrap_or_default(),
        Err(_) => vec![],
    };
    let ints_url = format!("{}/api/v1/integrations", api_base());
    let integrations: Vec<UiIntegration> = match cli
        .get(ints_url)
        .header("X-Auth-Token", token)
        .send()
        .await
        .and_then(|r| r.error_for_status())
    {
        Ok(resp) => resp.json().await.unwrap_or_default(),
        Err(_) => vec![],
    };
    // Webhooks via /api/v1/webhooks
    let hooks_url = format!("{}/api/v1/webhooks", api_base());
    let webhooks: Vec<UiWebhook> = match cli
        .get(hooks_url)
        .header("X-Auth-Token", &read_token_cookie(&headers).unwrap())
        .send()
        .await
        .and_then(|r| r.error_for_status())
    {
        Ok(resp) => resp.json().await.unwrap_or_default(),
        Err(_) => vec![],
    };
    // read current default_filter from cookie (fallback to "all")
    let default_filter_cookie =
        cookie_value(&headers, "default_filter").unwrap_or_else(|| "all".into());
    // read entries_per_page from /v1/me for hint
    let me_url = format!("{}/v1/me", api_base());
    let mut entries_per_page: i32 = 50;
    if let Ok(resp) = cli
        .get(me_url)
        .header("X-Auth-Token", &read_token_cookie(&headers).unwrap())
        .send()
        .await
        .and_then(|r| r.error_for_status())
    {
        #[derive(serde::Deserialize)]
        struct Me {
            entries_per_page: Option<i32>,
        }
        if let Ok(m) = resp.json::<Me>().await {
            if let Some(n) = m.entries_per_page {
                entries_per_page = n.max(1);
            }
        }
    }
    let def_filter = Box::leak(default_filter_cookie.into_boxed_str());
    let tpl = SettingsPage {
        title: "Settings",
        apikeys: &apikeys,
        integrations: &integrations,
        webhooks: &webhooks,
        dict: &dict,
        lang: &lang,
        default_filter: def_filter,
        entries_per_page,
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

async fn ui_prefs_language(headers: HeaderMap, body: Bytes) -> impl IntoResponse {
    let lang = {
        let mut out = None;
        for (k, v) in url::form_urlencoded::parse(&body) {
            if k == "lang" {
                out = Some(v.to_string());
            }
        }
        out.unwrap_or_else(|| "en_US".into())
    };
    // Try update user prefs via /v1/me -> id, then PUT /v1/users/{id}
    if let Some(token) = read_token_cookie(&headers) {
        let cli = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap();
        let me = format!("{}/v1/me", api_base());
        if let Ok(resp) = cli
            .get(me)
            .header("X-Auth-Token", &token)
            .send()
            .await
            .and_then(|r| r.error_for_status())
        {
            #[derive(serde::Deserialize)]
            struct Me {
                id: i64,
            }
            if let Ok(m) = resp.json::<Me>().await {
                let url = format!("{}/v1/users/{}", api_base(), m.id);
                let _ = cli
                    .put(url)
                    .header("X-Auth-Token", token)
                    .json(&serde_json::json!({"language": lang}))
                    .send()
                    .await;
            }
        }
    }
    // Always set cookie for immediate effect
    let res = axum::response::Response::builder()
        .status(axum::http::StatusCode::SEE_OTHER)
        .header(axum::http::header::LOCATION, "/settings")
        .header(
            axum::http::header::SET_COOKIE,
            format!("lang={}; Path=/; SameSite=Lax", lang),
        );
    res.body(axum::body::Body::empty())
        .unwrap_or_else(|_| axum::response::Response::new(axum::body::Body::empty()))
}

async fn ui_prefs_default_filter(_headers: HeaderMap, body: Bytes) -> impl IntoResponse {
    // Accept values: all | unread | starred
    let value = {
        let mut out: Option<String> = None;
        for (k, v) in url::form_urlencoded::parse(&body) {
            if k == "filter" {
                out = Some(v.to_string());
            }
        }
        let v = out.unwrap_or_else(|| "all".into()).to_ascii_lowercase();
        match v.as_str() {
            "unread" => "unread",
            "starred" => "starred",
            _ => "all",
        }
        .to_string()
    };
    let res = axum::response::Response::builder()
        .status(axum::http::StatusCode::SEE_OTHER)
        .header(axum::http::header::LOCATION, "/settings")
        .header(
            axum::http::header::SET_COOKIE,
            format!("default_filter={}; Path=/; SameSite=Lax", value),
        );
    res.body(axum::body::Body::empty())
        .unwrap_or_else(|_| axum::response::Response::new(axum::body::Body::empty()))
}

async fn ui_prefs_entries_per_page(headers: HeaderMap, body: Bytes) -> impl IntoResponse {
    let Some(token) = read_token_cookie(&headers) else {
        return Redirect::to("/login").into_response();
    };
    // parse positive integer 1..=200
    let mut num: i32 = 50;
    for (k, v) in url::form_urlencoded::parse(&body) {
        if k == "entries_per_page" {
            if let Ok(n) = v.parse::<i32>() {
                num = n.clamp(1, 200);
            }
        }
    }
    // PUT /v1/users/{id}
    let cli = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    let me = format!("{}/v1/me", api_base());
    if let Ok(resp) = cli
        .get(&me)
        .header("X-Auth-Token", &token)
        .send()
        .await
        .and_then(|r| r.error_for_status())
    {
        #[derive(serde::Deserialize)]
        struct Me {
            id: i64,
        }
        if let Ok(m) = resp.json::<Me>().await {
            let url = format!("{}/v1/users/{}", api_base(), m.id);
            let _ = cli
                .put(url)
                .header("X-Auth-Token", token)
                .json(&serde_json::json!({"entries_per_page": num}))
                .send()
                .await;
        }
    }
    Redirect::to("/settings").into_response()
}

async fn ui_prefs_sort_direction(headers: HeaderMap, body: Bytes) -> impl IntoResponse {
    let Some(token) = read_token_cookie(&headers) else {
        return Redirect::to("/login").into_response();
    };
    let mut dir = String::from("desc");
    for (k, v) in url::form_urlencoded::parse(&body) {
        if k == "entry_sorting_direction" {
            let s = v.to_string().to_ascii_lowercase();
            if s == "asc" || s == "desc" {
                dir = s;
            }
        }
    }
    let cli = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    let me = format!("{}/v1/me", api_base());
    if let Ok(resp) = cli
        .get(&me)
        .header("X-Auth-Token", &token)
        .send()
        .await
        .and_then(|r| r.error_for_status())
    {
        #[derive(serde::Deserialize)]
        struct Me {
            id: i64,
        }
        if let Ok(m) = resp.json::<Me>().await {
            let url = format!("{}/v1/users/{}", api_base(), m.id);
            let _ = cli
                .put(url)
                .header("X-Auth-Token", token)
                .json(&serde_json::json!({"entry_sorting_direction": dir}))
                .send()
                .await;
        }
    }
    Redirect::to("/settings").into_response()
}

async fn ui_prefs_keyboard_shortcuts(headers: HeaderMap, body: Bytes) -> impl IntoResponse {
    let Some(token) = read_token_cookie(&headers) else {
        return Redirect::to("/login").into_response();
    };
    let mut enabled = false;
    for (k, v) in url::form_urlencoded::parse(&body) {
        if k == "keyboard_shortcuts" {
            let s = v.to_string();
            enabled = s == "on" || s == "1" || s.eq_ignore_ascii_case("true");
        }
    }
    let cli = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    let me = format!("{}/v1/me", api_base());
    if let Ok(resp) = cli
        .get(&me)
        .header("X-Auth-Token", &token)
        .send()
        .await
        .and_then(|r| r.error_for_status())
    {
        #[derive(serde::Deserialize)]
        struct Me {
            id: i64,
        }
        if let Ok(m) = resp.json::<Me>().await {
            let url = format!("{}/v1/users/{}", api_base(), m.id);
            let _ = cli
                .put(url)
                .header("X-Auth-Token", token)
                .json(&serde_json::json!({"keyboard_shortcuts": enabled}))
                .send()
                .await;
        }
    }
    Redirect::to("/settings").into_response()
}

async fn ui_prefs_show_reading_time(headers: HeaderMap, body: Bytes) -> impl IntoResponse {
    let Some(token) = read_token_cookie(&headers) else {
        return Redirect::to("/login").into_response();
    };
    let mut enabled = false;
    for (k, v) in url::form_urlencoded::parse(&body) {
        if k == "show_reading_time" {
            let s = v.to_string();
            enabled = s == "on" || s == "1" || s.eq_ignore_ascii_case("true");
        }
    }
    let cli = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    let me = format!("{}/v1/me", api_base());
    if let Ok(resp) = cli
        .get(&me)
        .header("X-Auth-Token", &token)
        .send()
        .await
        .and_then(|r| r.error_for_status())
    {
        #[derive(serde::Deserialize)]
        struct Me {
            id: i64,
        }
        if let Ok(m) = resp.json::<Me>().await {
            let url = format!("{}/v1/users/{}", api_base(), m.id);
            let _ = cli
                .put(url)
                .header("X-Auth-Token", token)
                .json(&serde_json::json!({"show_reading_time": enabled}))
                .send()
                .await;
        }
    }
    Redirect::to("/settings").into_response()
}

async fn ui_prefs_open_ext_newtab(headers: HeaderMap, body: Bytes) -> impl IntoResponse {
    // Set cookie for immediate effect; attempt to persist to user prefs if supported
    let enabled = {
        let mut b = false;
        for (k, v) in url::form_urlencoded::parse(&body) {
            if k == "open_newtab" {
                let s = v.to_string();
                b = s == "on" || s == "1" || s.eq_ignore_ascii_case("true");
            }
        }
        b
    };
    if let Some(token) = read_token_cookie(&headers) {
        let cli = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap();
        let me = format!("{}/v1/me", api_base());
        if let Ok(resp) = cli
            .get(&me)
            .header("X-Auth-Token", &token)
            .send()
            .await
            .and_then(|r| r.error_for_status())
        {
            #[derive(serde::Deserialize)]
            struct Me {
                id: i64,
            }
            if let Ok(m) = resp.json::<Me>().await {
                let url = format!("{}/v1/users/{}", api_base(), m.id);
                let _ = cli
                    .put(url)
                    .header("X-Auth-Token", token)
                    .json(&serde_json::json!({"open_external_links_in_new_tab": enabled}))
                    .send()
                    .await;
            }
        }
    }
    let res = axum::response::Response::builder()
        .status(axum::http::StatusCode::SEE_OTHER)
        .header(axum::http::header::LOCATION, "/settings")
        .header(
            axum::http::header::SET_COOKIE,
            format!(
                "open_ext_newtab={}; Path=/; SameSite=Lax",
                if enabled { "1" } else { "0" }
            ),
        );
    res.body(axum::body::Body::empty())
        .unwrap_or_else(|_| axum::response::Response::new(axum::body::Body::empty()))
}

async fn ui_prefs_theme(headers: HeaderMap, body: Bytes) -> impl IntoResponse {
    let theme = {
        let mut out = String::from("system");
        for (k, v) in url::form_urlencoded::parse(&body) {
            if k == "theme" {
                let s = v.to_string();
                if s == "light" || s == "dark" || s == "system" {
                    out = s;
                }
            }
        }
        out
    };
    // persist to server theme if possible
    if let Some(token) = read_token_cookie(&headers) {
        let cli = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap();
        let me = format!("{}/v1/me", api_base());
        if let Ok(resp) = cli
            .get(&me)
            .header("X-Auth-Token", &token)
            .send()
            .await
            .and_then(|r| r.error_for_status())
        {
            #[derive(serde::Deserialize)]
            struct Me {
                id: i64,
            }
            if let Ok(m) = resp.json::<Me>().await {
                let url = format!("{}/v1/users/{}", api_base(), m.id);
                let server_theme = match theme.as_str() {
                    "light" => "light_serif",
                    "dark" => "dark_serif",
                    _ => "system_serif",
                };
                let _ = cli
                    .put(url)
                    .header("X-Auth-Token", token)
                    .json(&serde_json::json!({"theme": server_theme}))
                    .send()
                    .await;
            }
        }
    }
    let res = axum::response::Response::builder()
        .status(axum::http::StatusCode::SEE_OTHER)
        .header(axum::http::header::LOCATION, "/settings")
        .header(
            axum::http::header::SET_COOKIE,
            format!("theme={}; Path=/; SameSite=Lax", theme),
        );
    res.body(axum::body::Body::empty())
        .unwrap_or_else(|_| axum::response::Response::new(axum::body::Body::empty()))
}

async fn ui_prefs_custom_css(headers: HeaderMap, body: Bytes) -> impl IntoResponse {
    let Some(token) = read_token_cookie(&headers) else {
        return Redirect::to("/login").into_response();
    };
    let mut css = String::new();
    let mut font_hosts = None::<String>;
    for (k, v) in url::form_urlencoded::parse(&body) {
        if k == "custom_css" {
            css = v.to_string();
        } else if k == "external_font_hosts" {
            let s = v.to_string();
            if !s.trim().is_empty() {
                font_hosts = Some(s);
            } else {
                font_hosts = Some(String::new());
            }
        }
    }
    let cli = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    let me = format!("{}/v1/me", api_base());
    if let Ok(resp) = cli
        .get(&me)
        .header("X-Auth-Token", &token)
        .send()
        .await
        .and_then(|r| r.error_for_status())
    {
        #[derive(serde::Deserialize)]
        struct Me {
            id: i64,
        }
        if let Ok(m) = resp.json::<Me>().await {
            let url = format!("{}/v1/users/{}", api_base(), m.id);
            let mut payload = serde_json::json!({ "stylesheet": css });
            if let Some(hosts) = font_hosts {
                if let Some(obj) = payload.as_object_mut() {
                    obj.insert("external_font_hosts".to_string(), serde_json::json!(hosts));
                }
            }
            let _ = cli
                .put(url)
                .header("X-Auth-Token", token)
                .json(&payload)
                .send()
                .await;
        }
    }
    Redirect::to("/settings").into_response()
}

async fn ui_prefs_custom_js(headers: HeaderMap, body: Bytes) -> impl IntoResponse {
    let Some(token) = read_token_cookie(&headers) else {
        return Redirect::to("/login").into_response();
    };
    let mut js = String::new();
    for (k, v) in url::form_urlencoded::parse(&body) {
        if k == "custom_js" {
            js = v.to_string();
        }
    }
    let cli = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    let me = format!("{}/v1/me", api_base());
    if let Ok(resp) = cli
        .get(&me)
        .header("X-Auth-Token", &token)
        .send()
        .await
        .and_then(|r| r.error_for_status())
    {
        #[derive(serde::Deserialize)]
        struct Me {
            id: i64,
        }
        if let Ok(m) = resp.json::<Me>().await {
            let url = format!("{}/v1/users/{}", api_base(), m.id);
            let _ = cli
                .put(url)
                .header("X-Auth-Token", token)
                .json(&serde_json::json!({ "custom_js": js }))
                .send()
                .await;
        }
    }
    Redirect::to("/settings").into_response()
}

fn set_cookie_redirect(_headers: HeaderMap, name: &str, on: bool) -> axum::response::Response {
    let res = axum::response::Response::builder()
        .status(axum::http::StatusCode::SEE_OTHER)
        .header(axum::http::header::LOCATION, "/settings")
        .header(
            axum::http::header::SET_COOKIE,
            format!(
                "{}={}; Path=/; SameSite=Lax",
                name,
                if on { "1" } else { "0" }
            ),
        );
    res.body(axum::body::Body::empty())
        .unwrap_or_else(|_| axum::response::Response::new(axum::body::Body::empty()))
}

async fn ui_prefs_compact_ui(headers: HeaderMap, body: Bytes) -> impl IntoResponse {
    let mut on = false;
    for (k, v) in url::form_urlencoded::parse(&body) {
        if k == "compact_ui" {
            let s = v.to_string();
            on = s == "on" || s == "1" || s.eq_ignore_ascii_case("true");
        }
    }
    set_cookie_redirect(headers, "compact_ui", on)
}

async fn ui_prefs_minimal_ui(headers: HeaderMap, body: Bytes) -> impl IntoResponse {
    let mut on = false;
    for (k, v) in url::form_urlencoded::parse(&body) {
        if k == "minimal_ui" {
            let s = v.to_string();
            on = s == "on" || s == "1" || s.eq_ignore_ascii_case("true");
        }
    }
    set_cookie_redirect(headers, "minimal_ui", on)
}

async fn ui_prefs_auto_mark_read(headers: HeaderMap, body: Bytes) -> impl IntoResponse {
    let mut on = false;
    for (k, v) in url::form_urlencoded::parse(&body) {
        if k == "auto_mark_read" {
            let s = v.to_string();
            on = s == "on" || s == "1" || s.eq_ignore_ascii_case("true");
        }
    }
    // Persist to server mark_read_on_view if possible
    if let Some(token) = read_token_cookie(&headers) {
        let cli = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap();
        let me = format!("{}/v1/me", api_base());
        if let Ok(resp) = cli
            .get(&me)
            .header("X-Auth-Token", &token)
            .send()
            .await
            .and_then(|r| r.error_for_status())
        {
            #[derive(serde::Deserialize)]
            struct Me {
                id: i64,
            }
            if let Ok(m) = resp.json::<Me>().await {
                let url = format!("{}/v1/users/{}", api_base(), m.id);
                let _ = cli
                    .put(url)
                    .header("X-Auth-Token", token)
                    .json(&serde_json::json!({ "mark_read_on_view": on }))
                    .send()
                    .await;
            }
        }
    }
    set_cookie_redirect(headers, "auto_mark_read", on)
}

async fn ui_opml_export(headers: HeaderMap) -> impl IntoResponse {
    let Some(token) = read_token_cookie(&headers) else {
        return Redirect::to("/login").into_response();
    };
    let cli = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap();
    let url = format!("{}/v1/export", api_base());
    match cli
        .get(url)
        .header("X-Auth-Token", token)
        .send()
        .await
        .and_then(|r| r.error_for_status())
    {
        Ok(resp) => {
            let bytes = resp.bytes().await.unwrap_or_default();
            let mut res = axum::response::Response::builder().status(200);
            res = res.header(axum::http::header::CONTENT_TYPE, "text/xml; charset=utf-8");
            res = res.header(
                axum::http::header::CONTENT_DISPOSITION,
                "attachment; filename=feeds.opml",
            );
            res.body(axum::body::Body::from(bytes))
                .unwrap_or_else(|_| axum::response::Response::new(axum::body::Body::from("")))
        }
        Err(_) => Redirect::to("/settings").into_response(),
    }
}

async fn ui_opml_import(headers: HeaderMap, body: Bytes) -> impl IntoResponse {
    let Some(token) = read_token_cookie(&headers) else {
        return Redirect::to("/login").into_response();
    };
    let parsed = url::form_urlencoded::parse(&body);
    let mut content = String::new();
    for (k, v) in parsed {
        if k == "content" {
            content = v.to_string();
        }
    }
    if content.trim().is_empty() {
        return Redirect::to("/settings").into_response();
    }
    let cli = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .unwrap();
    let url = format!("{}/v1/import", api_base());
    let _ = cli
        .post(url)
        .header("X-Auth-Token", token)
        .header(axum::http::header::CONTENT_TYPE, "application/xml")
        .body(content)
        .send()
        .await;
    Redirect::to("/settings").into_response()
}

async fn ui_apikey_create(headers: HeaderMap, body: Bytes) -> impl IntoResponse {
    let Some(token) = read_token_cookie(&headers) else {
        return Redirect::to("/login").into_response();
    };
    let parsed = url::form_urlencoded::parse(&body);
    let mut desc = None;
    for (k, v) in parsed {
        if k == "description" {
            desc = Some(v.to_string());
        }
    }
    let payload = serde_json::json!({"description": desc});
    let cli = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    let url = format!("{}/v1/api-keys", api_base());
    let _ = cli
        .post(url)
        .header("X-Auth-Token", token)
        .json(&payload)
        .send()
        .await;
    Redirect::to("/settings").into_response()
}

async fn ui_apikey_delete(Path(id): Path<i64>, headers: HeaderMap) -> impl IntoResponse {
    let Some(token) = read_token_cookie(&headers) else {
        return Redirect::to("/login").into_response();
    };
    let cli = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    let url = format!("{}/v1/api-keys/{}", api_base(), id);
    let _ = cli.delete(url).header("X-Auth-Token", token).send().await;
    Redirect::to("/settings").into_response()
}

async fn ui_integration_create(headers: HeaderMap, body: Bytes) -> impl IntoResponse {
    let Some(token) = read_token_cookie(&headers) else {
        return Redirect::to("/login").into_response();
    };
    let mut kind = String::new();
    let mut enabled = true;
    let mut cfg = String::new();
    for (k, v) in url::form_urlencoded::parse(&body) {
        match &*k {
            "kind" => kind = v.to_string(),
            "enabled" => enabled = v == "on" || v == "1" || v.eq_ignore_ascii_case("true"),
            "config_json" => cfg = v.to_string(),
            _ => {}
        }
    }
    let config_json: serde_json::Value =
        serde_json::from_str(&cfg).unwrap_or(serde_json::json!({}));
    let payload = serde_json::json!({"kind": kind, "enabled": enabled, "config_json": config_json});
    let cli = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .unwrap();
    let url = format!("{}/api/v1/integrations", api_base());
    let _ = cli
        .post(url)
        .header("X-Auth-Token", token)
        .json(&payload)
        .send()
        .await;
    Redirect::to("/settings").into_response()
}

async fn ui_integration_update(
    Path(id): Path<i64>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let Some(token) = read_token_cookie(&headers) else {
        return Redirect::to("/login").into_response();
    };
    let mut enabled = None;
    let mut cfg = None;
    for (k, v) in url::form_urlencoded::parse(&body) {
        match &*k {
            "enabled" => enabled = Some(v == "on" || v == "1" || v.eq_ignore_ascii_case("true")),
            "config_json" => cfg = Some(v.to_string()),
            _ => {}
        }
    }
    let config_json = cfg.and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok());
    let mut payload = serde_json::Map::new();
    if let Some(b) = enabled {
        payload.insert("enabled".into(), serde_json::Value::Bool(b));
    }
    if let Some(j) = config_json {
        payload.insert("config_json".into(), j);
    }
    let cli = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .unwrap();
    let url = format!("{}/api/v1/integrations/{}", api_base(), id);
    let _ = cli
        .put(url)
        .header("X-Auth-Token", token)
        .json(&payload)
        .send()
        .await;
    Redirect::to("/settings").into_response()
}

async fn ui_integration_delete(Path(id): Path<i64>, headers: HeaderMap) -> impl IntoResponse {
    let Some(token) = read_token_cookie(&headers) else {
        return Redirect::to("/login").into_response();
    };
    let cli = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    let url = format!("{}/api/v1/integrations/{}", api_base(), id);
    let _ = cli.delete(url).header("X-Auth-Token", token).send().await;
    Redirect::to("/settings").into_response()
}

async fn ui_webhook_create(headers: HeaderMap, body: Bytes) -> impl IntoResponse {
    let Some(token) = read_token_cookie(&headers) else {
        return Redirect::to("/login").into_response();
    };
    let mut urlv = String::new();
    let mut events = None;
    for (k, v) in url::form_urlencoded::parse(&body) {
        match &*k {
            "url" => urlv = v.to_string(),
            "events" => {
                let s = v.to_string();
                if !s.trim().is_empty() {
                    events = Some(s);
                }
            }
            _ => {}
        }
    }
    if !urlv.trim().is_empty() {
        let payload = serde_json::json!({"url": urlv, "events": events});
        let cli = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(8))
            .build()
            .unwrap();
        let api = format!("{}/api/v1/webhooks", api_base());
        let _ = cli
            .post(api)
            .header("X-Auth-Token", token)
            .json(&payload)
            .send()
            .await;
    }
    Redirect::to("/settings").into_response()
}

async fn ui_webhook_delete(Path(id): Path<i64>, headers: HeaderMap) -> impl IntoResponse {
    let Some(token) = read_token_cookie(&headers) else {
        return Redirect::to("/login").into_response();
    };
    let cli = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();
    let api = format!("{}/api/v1/webhooks/{}", api_base(), id);
    let _ = cli.delete(api).header("X-Auth-Token", token).send().await;
    Redirect::to("/settings").into_response()
}
#[derive(Deserialize, Clone)]
struct UiFeedFull {
    id: i64,
    title: Option<String>,
    #[serde(default)]
    category: Option<UiCategory>,
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
    categories: &'a [UiCategory],
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
    let categories: Vec<UiCategory> = match cli
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
