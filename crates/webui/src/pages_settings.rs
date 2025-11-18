use askama::Template;
use axum::{
    body::Bytes,
    extract::Path,
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
pub struct UiApiKey {
    pub id: i64,
    pub user_id: i64,
    pub token: String,
    pub description: Option<String>,
    pub last_used_at: Option<String>,
    pub created_at: String,
}

#[derive(Deserialize, Clone)]
pub struct UiIntegration {
    pub id: i64,
    pub kind: String,
    pub enabled: bool,
    pub config_json: Option<serde_json::Value>,
}

#[derive(Deserialize, Clone)]
#[allow(dead_code)]
pub struct UiWebhook {
    pub id: i64,
    pub url: String,
    pub events: Option<String>,
    pub enabled: bool,
    pub created_at: String,
}

#[derive(Template)]
#[template(path = "settings.html")]
pub struct SettingsPage<'a> {
    pub title: &'a str,
    pub apikeys: &'a [UiApiKey],
    pub integrations: &'a [UiIntegration],
    pub webhooks: &'a [UiWebhook],
    pub dict: &'a std::collections::HashMap<String, String>,
    pub lang: &'a str,
    pub default_filter: &'a str,
    pub entries_per_page: i32,
    pub csp_nonce: &'a str,
    pub custom_css: &'a str,
    pub custom_js: &'a str,
    pub external_font_hosts: &'a str,
}

pub async fn ui_settings(headers: HeaderMap) -> impl IntoResponse {
    let Some(token) = read_token_cookie(&headers) else {
        return Redirect::to("/login").into_response();
    };
    let lang = resolve_lang(&headers).await;
    let dict = i18n::load(&lang);
    let snippets = load_snippets(&headers).await;
    let nonce = gen_csp_nonce();
    let Some(cli) = http_client(5) else {
        return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "http client error",
        )
            .into_response();
    };
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
    // read entries_per_page from native /api/v1/me for hint
    let me_url = format!("{}/api/v1/me", api_base());
    let mut entries_per_page: i32 = 50;
    if let Ok(resp) = cli
        .get(me_url)
        .header(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {}", read_token_cookie(&headers).unwrap()),
        )
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
        Err(_) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "template error",
        )
            .into_response(),
    }
}

pub async fn ui_prefs_language(headers: HeaderMap, body: Bytes) -> impl IntoResponse {
    let lang = {
        let mut out = None;
        for (k, v) in url::form_urlencoded::parse(&body) {
            if k == "lang" {
                out = Some(v.to_string());
            }
        }
        out.unwrap_or_else(|| "en_US".into())
    };
    // Update user prefs via native /api/v1/me/prefs
    if let Some(token) = read_token_cookie(&headers) {
        let Some(cli) = http_client(5) else {
            return Redirect::to("/settings").into_response();
        };
        let url = format!("{}/api/v1/me/prefs", api_base());
        let _ = cli
            .put(url)
            .header(
                axum::http::header::AUTHORIZATION,
                format!("Bearer {}", token),
            )
            .json(&serde_json::json!({ "language": lang }))
            .send()
            .await;
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

pub async fn ui_prefs_default_filter(_headers: HeaderMap, body: Bytes) -> impl IntoResponse {
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

pub async fn ui_prefs_entries_per_page(headers: HeaderMap, body: Bytes) -> impl IntoResponse {
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
    let Some(cli) = http_client(5) else {
        return Redirect::to("/settings").into_response();
    };
    let url = format!("{}/api/v1/me/prefs", api_base());
    let _ = cli
        .put(url)
        .header(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {}", token),
        )
        .json(&serde_json::json!({"entries_per_page": num}))
        .send()
        .await;
    Redirect::to("/settings").into_response()
}

pub async fn ui_prefs_sort_direction(headers: HeaderMap, body: Bytes) -> impl IntoResponse {
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
    let Some(cli) = http_client(5) else {
        return Redirect::to("/settings").into_response();
    };
    let url = format!("{}/api/v1/me/prefs", api_base());
    let _ = cli
        .put(url)
        .header(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {}", token),
        )
        .json(&serde_json::json!({"entry_sorting_direction": dir}))
        .send()
        .await;
    Redirect::to("/settings").into_response()
}

pub async fn ui_prefs_keyboard_shortcuts(headers: HeaderMap, body: Bytes) -> impl IntoResponse {
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
    let Some(cli) = http_client(5) else {
        return Redirect::to("/settings").into_response();
    };
    let url = format!("{}/api/v1/me/prefs", api_base());
    let _ = cli
        .put(url)
        .header(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {}", token),
        )
        .json(&serde_json::json!({"keyboard_shortcuts": enabled}))
        .send()
        .await;
    Redirect::to("/settings").into_response()
}

pub async fn ui_prefs_show_reading_time(headers: HeaderMap, body: Bytes) -> impl IntoResponse {
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
    let Some(cli) = http_client(5) else {
        return Redirect::to("/settings").into_response();
    };
    let url = format!("{}/api/v1/me/prefs", api_base());
    let _ = cli
        .put(url)
        .header(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {}", token),
        )
        .json(&serde_json::json!({"show_reading_time": enabled}))
        .send()
        .await;
    Redirect::to("/settings").into_response()
}

pub async fn ui_prefs_open_ext_newtab(headers: HeaderMap, body: Bytes) -> impl IntoResponse {
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
        let Some(cli) = http_client(5) else {
            return Redirect::to("/settings").into_response();
        };
        let url = format!("{}/api/v1/me/prefs", api_base());
        let _ = cli
            .put(url)
            .header(
                axum::http::header::AUTHORIZATION,
                format!("Bearer {}", token),
            )
            .json(&serde_json::json!({"open_external_links_in_new_tab": enabled}))
            .send()
            .await;
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

pub async fn ui_prefs_theme(headers: HeaderMap, body: Bytes) -> impl IntoResponse {
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
        let Some(cli) = http_client(5) else {
            return Redirect::to("/settings").into_response();
        };
        let server_theme = match theme.as_str() {
            "light" => "light_serif",
            "dark" => "dark_serif",
            _ => "system_serif",
        };
        let url = format!("{}/api/v1/me/prefs", api_base());
        let _ = cli
            .put(url)
            .header(
                axum::http::header::AUTHORIZATION,
                format!("Bearer {}", token),
            )
            .json(&serde_json::json!({"theme": server_theme}))
            .send()
            .await;
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

pub async fn ui_prefs_custom_css(headers: HeaderMap, body: Bytes) -> impl IntoResponse {
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
    let Some(cli) = http_client(5) else {
        return Redirect::to("/settings").into_response();
    };
    let mut payload = serde_json::json!({ "stylesheet": css });
    if let Some(hosts) = font_hosts {
        if let Some(obj) = payload.as_object_mut() {
            obj.insert("external_font_hosts".to_string(), serde_json::json!(hosts));
        }
    }
    let url = format!("{}/api/v1/me/prefs", api_base());
    let _ = cli
        .put(url)
        .header(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {}", token),
        )
        .json(&payload)
        .send()
        .await;
    Redirect::to("/settings").into_response()
}

pub async fn ui_prefs_custom_js(headers: HeaderMap, body: Bytes) -> impl IntoResponse {
    let Some(token) = read_token_cookie(&headers) else {
        return Redirect::to("/login").into_response();
    };
    let mut js = String::new();
    for (k, v) in url::form_urlencoded::parse(&body) {
        if k == "custom_js" {
            js = v.to_string();
        }
    }
    let Some(cli) = http_client(5) else {
        return Redirect::to("/settings").into_response();
    };
    let url = format!("{}/api/v1/me/prefs", api_base());
    let _ = cli
        .put(url)
        .header(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {}", token),
        )
        .json(&serde_json::json!({ "custom_js": js }))
        .send()
        .await;
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

pub async fn ui_prefs_compact_ui(headers: HeaderMap, body: Bytes) -> impl IntoResponse {
    let mut on = false;
    for (k, v) in url::form_urlencoded::parse(&body) {
        if k == "compact_ui" {
            let s = v.to_string();
            on = s == "on" || s == "1" || s.eq_ignore_ascii_case("true");
        }
    }
    set_cookie_redirect(headers, "compact_ui", on)
}

pub async fn ui_prefs_minimal_ui(headers: HeaderMap, body: Bytes) -> impl IntoResponse {
    let mut on = false;
    for (k, v) in url::form_urlencoded::parse(&body) {
        if k == "minimal_ui" {
            let s = v.to_string();
            on = s == "on" || s == "1" || s.eq_ignore_ascii_case("true");
        }
    }
    set_cookie_redirect(headers, "minimal_ui", on)
}

pub async fn ui_prefs_auto_mark_read(headers: HeaderMap, body: Bytes) -> impl IntoResponse {
    let mut on = false;
    for (k, v) in url::form_urlencoded::parse(&body) {
        if k == "auto_mark_read" {
            let s = v.to_string();
            on = s == "on" || s == "1" || s.eq_ignore_ascii_case("true");
        }
    }
    // Persist to server mark_read_on_view if possible
    if let Some(token) = read_token_cookie(&headers) {
        let Some(cli) = http_client(5) else {
            return Redirect::to("/settings").into_response();
        };
        let url = format!("{}/api/v1/me/prefs", api_base());
        let _ = cli
            .put(url)
            .header(
                axum::http::header::AUTHORIZATION,
                format!("Bearer {}", token),
            )
            .json(&serde_json::json!({ "mark_read_on_view": on }))
            .send()
            .await;
    }
    set_cookie_redirect(headers, "auto_mark_read", on)
}

pub async fn ui_opml_export(headers: HeaderMap) -> impl IntoResponse {
    let Some(token) = read_token_cookie(&headers) else {
        return Redirect::to("/login").into_response();
    };
    let Some(cli) = http_client(10) else {
        return Redirect::to("/settings").into_response();
    };
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

pub async fn ui_opml_import(headers: HeaderMap, body: Bytes) -> impl IntoResponse {
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
    let Some(cli) = http_client(15) else {
        return Redirect::to("/settings").into_response();
    };
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

pub async fn ui_apikey_create(headers: HeaderMap, body: Bytes) -> impl IntoResponse {
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
    let Some(cli) = http_client(5) else {
        return Redirect::to("/settings").into_response();
    };
    let url = format!("{}/v1/api-keys", api_base());
    let _ = cli
        .post(url)
        .header("X-Auth-Token", token)
        .json(&payload)
        .send()
        .await;
    Redirect::to("/settings").into_response()
}

pub async fn ui_apikey_delete(Path(id): Path<i64>, headers: HeaderMap) -> impl IntoResponse {
    let Some(token) = read_token_cookie(&headers) else {
        return Redirect::to("/login").into_response();
    };
    let Some(cli) = http_client(5) else {
        return Redirect::to("/settings").into_response();
    };
    let url = format!("{}/v1/api-keys/{}", api_base(), id);
    let _ = cli.delete(url).header("X-Auth-Token", token).send().await;
    Redirect::to("/settings").into_response()
}

pub async fn ui_integration_create(headers: HeaderMap, body: Bytes) -> impl IntoResponse {
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
    let Some(cli) = http_client(8) else {
        return Redirect::to("/settings").into_response();
    };
    let url = format!("{}/api/v1/integrations", api_base());
    let _ = cli
        .post(url)
        .header("X-Auth-Token", token)
        .json(&payload)
        .send()
        .await;
    Redirect::to("/settings").into_response()
}

pub async fn ui_integration_update(
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
    let Some(cli) = http_client(8) else {
        return Redirect::to("/settings").into_response();
    };
    let url = format!("{}/api/v1/integrations/{}", api_base(), id);
    let _ = cli
        .put(url)
        .header("X-Auth-Token", token)
        .json(&payload)
        .send()
        .await;
    Redirect::to("/settings").into_response()
}

pub async fn ui_integration_delete(Path(id): Path<i64>, headers: HeaderMap) -> impl IntoResponse {
    let Some(token) = read_token_cookie(&headers) else {
        return Redirect::to("/login").into_response();
    };
    let Some(cli) = http_client(5) else {
        return Redirect::to("/settings").into_response();
    };
    let url = format!("{}/api/v1/integrations/{}", api_base(), id);
    let _ = cli.delete(url).header("X-Auth-Token", token).send().await;
    Redirect::to("/settings").into_response()
}

pub async fn ui_webhook_create(headers: HeaderMap, body: Bytes) -> impl IntoResponse {
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
        let Some(cli) = http_client(8) else {
            return Redirect::to("/settings").into_response();
        };
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

pub async fn ui_webhook_delete(Path(id): Path<i64>, headers: HeaderMap) -> impl IntoResponse {
    let Some(token) = read_token_cookie(&headers) else {
        return Redirect::to("/login").into_response();
    };
    let Some(cli) = http_client(5) else {
        return Redirect::to("/settings").into_response();
    };
    let api = format!("{}/api/v1/webhooks/{}", api_base(), id);
    let _ = cli.delete(api).header("X-Auth-Token", token).send().await;
    Redirect::to("/settings").into_response()
}
