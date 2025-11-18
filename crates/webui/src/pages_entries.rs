use askama::Template;
use axum::{
    extract::{Path, Query},
    http::HeaderMap,
    response::{Html, IntoResponse, Redirect},
};
use serde::Deserialize;

use crate::filters;
use crate::i18n;
use crate::util::{
    api_base, cookie_value, gen_csp_nonce, http_client, load_snippets, read_token_cookie,
    resolve_lang,
};

#[derive(Deserialize, Clone)]
#[allow(dead_code)]
pub struct UiEntryBrief {
    pub id: i64,
    pub title: Option<String>,
    pub url: Option<String>,
    pub author: Option<String>,
    #[serde(rename = "published_at")]
    pub date: Option<String>,
    pub starred: bool,
    pub status: String,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
pub struct UiEntrySet {
    pub total: i64,
    pub entries: Vec<UiEntryBrief>,
}

#[derive(Template)]
#[template(path = "entries.html")]
pub struct EntriesPage<'a> {
    pub title: &'a str,
    pub feed_id: i64,
    pub items: &'a [UiEntryBrief],
    pub limit: usize,
    pub prev_page: Option<usize>,
    pub next_page: Option<usize>,
    pub dict: &'a std::collections::HashMap<String, String>,
    pub filter: &'a str,
    pub filter_q: &'a str,
    pub search_q_qs: &'a str,
    pub search_q: &'a str,
    pub refreshed: bool,
    pub refresh_err: bool,
    pub csp_nonce: &'a str,
    pub custom_css: &'a str,
    pub custom_js: &'a str,
    pub external_font_hosts: &'a str,
}

#[derive(Deserialize, Default)]
pub struct UiListQuery {
    pub page: Option<usize>,
    pub limit: Option<usize>,
    pub status: Option<String>,
    pub starred: Option<bool>,
    pub q: Option<String>,
    pub refreshed: Option<bool>,
    pub refresh_err: Option<bool>,
}

pub async fn ui_feed_entries(
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
    let Some(cli) = http_client(4) else {
        return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "http client error",
        )
            .into_response();
    };
    let limit = if let Some(l) = q.limit {
        l.clamp(1, 200)
    } else {
        // fallback: use entries_per_page from native /api/v1/me
        let me_url = format!("{}/api/v1/me", api_base());
        match cli
            .get(&me_url)
            .header(
                axum::http::header::AUTHORIZATION,
                format!("Bearer {}", token),
            )
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
    // Use native `/api/v1/entries` as the canonical timeline endpoint, scoped
    // by `feed_id`. This aligns WebUI with Captura's unified timeline model
    // instead of Miniflux-compatible `/v1/feeds/{id}/entries`.
    let mut url = format!(
        "{}/api/v1/entries?feed_id={}&limit={}&offset={}&sort_by=published_at&order=desc&include_tags=true",
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
            url.push_str("&status=starred");
            filter = "starred".into();
            filter_q = "&status=starred".into();
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
        .header(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {}", token.clone()),
        )
        .send()
        .await;
    let api_entries: Vec<ApiSmartEntry> = match res.and_then(|r| r.error_for_status()) {
        Ok(resp) => resp.json().await.unwrap_or_default(),
        Err(_) => Vec::new(),
    };
    let mut items: Vec<UiEntryBrief> = Vec::with_capacity(api_entries.len());
    for e in api_entries {
        items.push(UiEntryBrief {
            id: e.id,
            title: e.title,
            url: e.url,
            author: e.author,
            date: e.date,
            starred: e.is_starred,
            status: if e.is_read {
                "read".into()
            } else {
                "unread".into()
            },
            tags: e.tags,
        });
    }
    let prev_page = if page > 1 { Some(page - 1) } else { None };
    // We do not know total count; show "next" when we filled the current page.
    let next_page = if items.len() >= limit {
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
        items: &items,
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
        Err(_) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "template error",
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
struct ApiSmartEntry {
    id: i64,
    title: Option<String>,
    url: Option<String>,
    author: Option<String>,
    #[serde(rename = "published_at")]
    date: Option<String>,
    is_read: bool,
    is_starred: bool,
    #[serde(default)]
    tags: Option<Vec<String>>,
}

#[derive(Template)]
#[template(path = "smart_view_entries.html")]
pub struct SmartEntriesPage<'a> {
    pub title: &'a str,
    pub smart_view: &'a crate::pages_feeds::UiSmartView,
    pub feeds: &'a [crate::SmartViewFeedOption],
    pub categories: &'a [crate::SmartViewCategoryOption],
    pub labels: &'a [crate::SmartViewLabelOption],
    pub items: &'a [UiEntryBrief],
    pub limit: usize,
    pub prev_page: Option<usize>,
    pub next_page: Option<usize>,
    pub dict: &'a std::collections::HashMap<String, String>,
    pub csp_nonce: &'a str,
    pub custom_css: &'a str,
    pub custom_js: &'a str,
    pub external_font_hosts: &'a str,
}

pub async fn ui_smart_view_entries(
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
    let Some(cli) = http_client(4) else {
        return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "http client error",
        )
            .into_response();
    };

    // Load smart view metadata
    let sv_url = format!("{}/api/v1/smart-views/{}", api_base(), id);
    let sv_res = cli
        .get(sv_url)
        .header(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {}", token.clone()),
        )
        .send()
        .await;
    let smart_view: crate::pages_feeds::UiSmartView =
        match sv_res.and_then(|r| r.error_for_status()) {
            Ok(resp) => match resp.json().await {
                Ok(v) => v,
                Err(_) => {
                    return Redirect::to("/feeds").into_response();
                }
            },
            Err(_) => {
                return Redirect::to("/feeds").into_response();
            }
        };

    // Load feeds/categories/labels for editing filters.
    let mut feeds: Vec<crate::SmartViewFeedOption> = Vec::new();
    let mut categories: Vec<crate::SmartViewCategoryOption> = Vec::new();
    let mut labels: Vec<crate::SmartViewLabelOption> = Vec::new();
    let feeds_url = format!("{}/api/v1/feeds?sort_by=title&order=asc", api_base());
    if let Ok(resp) = cli
        .get(&feeds_url)
        .header(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {}", token.clone()),
        )
        .send()
        .await
        .and_then(|r| r.error_for_status())
    {
        feeds = resp.json().await.unwrap_or_default();
    }
    let cats_url = format!("{}/api/v1/categories", api_base());
    if let Ok(resp) = cli
        .get(&cats_url)
        .header(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {}", token.clone()),
        )
        .send()
        .await
        .and_then(|r| r.error_for_status())
    {
        categories = resp.json().await.unwrap_or_default();
    }
    let labels_url = format!("{}/api/v1/labels", api_base());
    if let Ok(resp) = cli
        .get(&labels_url)
        .header(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {}", token.clone()),
        )
        .send()
        .await
        .and_then(|r| r.error_for_status())
    {
        labels = resp.json().await.unwrap_or_default();
    }

    // Determine pagination (reuse entries_per_page from /api/v1/me when limit is not provided)
    let limit = if let Some(l) = q.limit {
        l.clamp(1, 200)
    } else {
        let me_url = format!("{}/api/v1/me", api_base());
        match cli
            .get(me_url)
            .header(
                axum::http::header::AUTHORIZATION,
                format!("Bearer {}", token),
            )
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

    // Fetch entries for this smart view
    let url = format!(
        "{}/api/v1/smart-views/{}/entries?limit={}&offset={}",
        api_base(),
        id,
        limit,
        offset
    );
    let res = cli
        .get(url)
        .header(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {}", token),
        )
        .send()
        .await;
    let api_entries: Vec<ApiSmartEntry> = match res.and_then(|r| r.error_for_status()) {
        Ok(resp) => resp.json().await.unwrap_or_default(),
        Err(_) => Vec::new(),
    };
    let mut items: Vec<UiEntryBrief> = Vec::with_capacity(api_entries.len());
    for e in api_entries {
        items.push(UiEntryBrief {
            id: e.id,
            title: e.title,
            url: e.url,
            author: e.author,
            date: e.date,
            starred: e.is_starred,
            status: if e.is_read {
                "read".into()
            } else {
                "unread".into()
            },
            tags: None,
        });
    }

    let prev_page = if page > 1 { Some(page - 1) } else { None };
    // We do not know total count; show "next" when we filled the current page.
    let next_page = if items.len() >= limit {
        Some(page + 1)
    } else {
        None
    };

    let tpl = SmartEntriesPage {
        title: "Entries",
        smart_view: &smart_view,
        feeds: &feeds,
        categories: &categories,
        labels: &labels,
        items: &items,
        limit,
        prev_page,
        next_page,
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

#[derive(Deserialize)]
pub struct UiEntryFull {
    pub id: i64,
    pub title: Option<String>,
    pub author: Option<String>,
    pub url: Option<String>,
    pub content: Option<String>,
    pub status: String,
    pub starred: bool,
    pub feed_id: i64,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
}

#[derive(Template)]
#[template(path = "entry.html")]
pub struct EntryPage<'a> {
    pub title: &'a str,
    pub entry: &'a UiEntryFull,
    pub prev_id: Option<i64>,
    pub next_id: Option<i64>,
    pub dict: &'a std::collections::HashMap<String, String>,
    pub csp_nonce: &'a str,
    pub custom_css: &'a str,
    pub custom_js: &'a str,
    pub external_font_hosts: &'a str,
}

pub async fn ui_entry(Path(id): Path<i64>, headers: HeaderMap) -> impl IntoResponse {
    let Some(token) = read_token_cookie(&headers) else {
        return Redirect::to("/login").into_response();
    };
    let lang = resolve_lang(&headers).await;
    let dict = i18n::load(&lang);
    let snippets = load_snippets(&headers).await;
    let nonce = gen_csp_nonce();
    let Some(cli) = http_client(4) else {
        return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "http client error",
        )
            .into_response();
    };
    let url = format!("{}/api/v1/entries/{}", api_base(), id);
    let res = cli
        .get(url)
        .header(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {}", token.clone()),
        )
        .send()
        .await;
    #[derive(serde::Deserialize)]
    #[allow(dead_code)]
    struct ApiEntryDto {
        id: i64,
        feed_id: i64,
        url: Option<String>,
        title: Option<String>,
        summary: Option<String>,
        content_html: Option<String>,
        author: Option<String>,
        published_at: Option<String>,
        is_read: bool,
        is_starred: bool,
        #[serde(default)]
        tags: Option<Vec<String>>,
    }
    let entry: UiEntryFull = match res.and_then(|r| r.error_for_status()) {
        Ok(resp) => match resp.json::<ApiEntryDto>().await {
            Ok(e) => UiEntryFull {
                id: e.id,
                title: e.title,
                author: e.author,
                url: e.url,
                content: e.content_html.or(e.summary),
                status: if e.is_read {
                    "read".into()
                } else {
                    "unread".into()
                },
                starred: e.is_starred,
                feed_id: e.feed_id,
                tags: e.tags,
            },
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
        },
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
        // prev: before_id current (id-based cursor on native timeline)
        let prev_url = format!(
            "{}/api/v1/entries?feed_id={}&before_id={}&sort_by=id&order=desc&limit=1",
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
            "{}/api/v1/entries?feed_id={}&after_id={}&sort_by=id&order=asc&limit=1",
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
        Err(_) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "template error",
        )
            .into_response(),
    }
}
