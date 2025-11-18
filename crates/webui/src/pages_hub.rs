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

#[derive(Template)]
#[template(path = "hub_routes.html")]
pub struct HubRoutesPage<'a> {
    pub title: &'a str,
    pub groups: &'a [UiHubNamespaceGroup],
    pub dict: &'a std::collections::HashMap<String, String>,
    pub csp_nonce: &'a str,
    pub custom_css: &'a str,
    pub custom_js: &'a str,
    pub external_font_hosts: &'a str,
}

#[derive(Template)]
#[template(path = "hub_test.html")]
pub struct HubTestPage<'a> {
    pub title: &'a str,
    pub preview: Option<UiHubPreview>,
    pub preview_url: &'a str,
    pub dict: &'a std::collections::HashMap<String, String>,
    pub csp_nonce: &'a str,
    pub custom_css: &'a str,
    pub custom_js: &'a str,
    pub external_font_hosts: &'a str,
}

#[derive(Template)]
#[template(path = "rules_test.html")]
pub struct RulesTestPage<'a> {
    pub title: &'a str,
    pub url: &'a str,
    pub yaml: &'a Option<String>,
    pub result: &'a Option<UiTryRuleResp>,
    pub dict: &'a std::collections::HashMap<String, String>,
    pub csp_nonce: &'a str,
    pub custom_css: &'a str,
    pub custom_js: &'a str,
    pub external_font_hosts: &'a str,
}

#[derive(Template)]
#[template(path = "hub_stats.html")]
pub struct HubStatsPage<'a> {
    pub title: &'a str,
    pub rules: &'a [UiRuleStats],
    pub hubs: &'a [UiHubRouteStats],
    pub dict: &'a std::collections::HashMap<String, String>,
    pub csp_nonce: &'a str,
    pub custom_css: &'a str,
    pub custom_js: &'a str,
    pub external_font_hosts: &'a str,
}

#[allow(dead_code)]
#[derive(Deserialize, Clone)]
pub struct UiHubRoute {
    pub hub_id: String,
    pub path: String,
    pub categories: Vec<String>,
    pub example: String,
    #[serde(default)]
    pub parameters: Vec<(String, String)>,
    pub name: String,
    pub url: String,
    pub description: String,
}

#[derive(Clone)]
pub struct UiHubNamespaceGroup {
    pub namespace: String,
    pub routes: Vec<UiHubRoute>,
}

#[allow(dead_code)]
#[derive(Deserialize, Clone)]
pub struct UiHubItem {
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub link: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
}

#[allow(dead_code)]
#[derive(Deserialize, Clone)]
pub struct UiHubPreview {
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub link: Option<String>,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    pub items: Vec<UiHubItem>,
}

#[derive(Deserialize, Clone)]
pub struct UiTryRuleEntry {
    pub title: Option<String>,
    pub url: Option<String>,
    pub content_len: usize,
}

#[derive(Deserialize, Clone)]
pub struct UiTryRuleResp {
    pub used_smart: bool,
    pub item_count: usize,
    pub entries: Vec<UiTryRuleEntry>,
    pub duration_ms: u128,
}

#[derive(Deserialize, Default)]
pub struct UiHubQuery {
    url: Option<String>,
}

#[derive(Deserialize, Clone)]
pub struct UiRuleStats {
    pub id: i64,
    pub rule_id: String,
    #[serde(default)]
    pub description: Option<String>,
    pub total_jobs: i64,
    pub done_jobs: i64,
    pub failed_jobs: i64,
    #[serde(default)]
    pub last_error: Option<String>,
}

#[derive(Deserialize, Clone)]
pub struct UiHubRouteStats {
    pub hub_id: String,
    pub total_jobs: i64,
    pub done_jobs: i64,
    pub failed_jobs: i64,
    #[serde(default)]
    pub last_error: Option<String>,
}

pub async fn ui_hub_routes(headers: HeaderMap, _q: Query<UiHubQuery>) -> impl IntoResponse {
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

    #[derive(Deserialize)]
    struct HubRoutesResp {
        routes: Vec<UiHubRoute>,
    }

    let routes_url = format!("{}/api/v1/hub/routes", api_base());
    let routes_flat: Vec<UiHubRoute> = match cli
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

    use std::collections::BTreeMap;
    let mut by_ns: BTreeMap<String, Vec<UiHubRoute>> = BTreeMap::new();
    for r in routes_flat {
        let ns = r.hub_id.split('/').next().unwrap_or("").to_string();
        by_ns.entry(ns).or_default().push(r);
    }
    let mut groups: Vec<UiHubNamespaceGroup> = by_ns
        .into_iter()
        .map(|(namespace, mut routes)| {
            routes.sort_by(|a, b| a.hub_id.cmp(&b.hub_id));
            UiHubNamespaceGroup { namespace, routes }
        })
        .collect();
    groups.sort_by(|a, b| a.namespace.cmp(&b.namespace));

    let tpl = HubRoutesPage {
        title: "Hub Routes",
        groups: &groups,
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

pub async fn ui_hub_stats(headers: HeaderMap) -> impl IntoResponse {
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

    let mut rules: Vec<UiRuleStats> = Vec::new();
    let mut hubs: Vec<UiHubRouteStats> = Vec::new();

    let rules_url = format!("{}/api/v1/rules/stats", api_base());
    if let Ok(resp) = cli
        .get(&rules_url)
        .header(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {}", token),
        )
        .send()
        .await
        .and_then(|r| r.error_for_status())
    {
        rules = resp.json().await.unwrap_or_default();
    }

    let hubs_url = format!("{}/api/v1/hub/routes/stats", api_base());
    if let Ok(resp) = cli
        .get(&hubs_url)
        .header(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {}", token),
        )
        .send()
        .await
        .and_then(|r| r.error_for_status())
    {
        hubs = resp.json().await.unwrap_or_default();
    }

    let tpl = HubStatsPage {
        title: "Hub & Rules Stats",
        rules: &rules,
        hubs: &hubs,
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

pub async fn ui_hub_test(headers: HeaderMap, Query(q): Query<UiHubQuery>) -> impl IntoResponse {
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

    let tpl = HubTestPage {
        title: "Hub Preview",
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
        Err(_) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "template error",
        )
            .into_response(),
    }
}

use axum::Form;

#[derive(Deserialize)]
pub struct RulesTestForm {
    url: String,
    #[serde(default)]
    yaml: String,
}

pub async fn ui_rules_test(
    headers: HeaderMap,
    Form(form): Form<RulesTestForm>,
) -> impl IntoResponse {
    let Some(token) = read_token_cookie(&headers) else {
        return Redirect::to("/login").into_response();
    };
    let lang = resolve_lang(&headers).await;
    let dict = i18n::load(&lang);
    let snippets = load_snippets(&headers).await;
    let nonce = gen_csp_nonce();

    let url = form.url.clone();
    let yaml = if form.yaml.trim().is_empty() {
        None
    } else {
        Some(form.yaml.clone())
    };
    let mut result: Option<UiTryRuleResp> = None;

    if !url.trim().is_empty() {
        if let Some(yaml_str) = yaml.as_ref() {
            let Some(cli) = http_client(6) else {
                return (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    "http client error",
                )
                    .into_response();
            };
            #[derive(serde::Serialize)]
            struct TryReq<'a> {
                url: &'a str,
                yaml: &'a str,
            }
            let body = TryReq {
                url: &url,
                yaml: yaml_str,
            };
            let endpoint = format!("{}/api/v1/rules/try", api_base());
            if let Ok(resp) = cli
                .post(endpoint)
                .header("Authorization", format!("Bearer {}", token))
                .json(&body)
                .send()
                .await
                .and_then(|r| r.error_for_status())
            {
                if let Ok(r) = resp.json::<UiTryRuleResp>().await {
                    result = Some(r);
                }
            }
        }
    }

    let tpl = RulesTestPage {
        title: "Test Rule",
        url: &url,
        yaml: &yaml,
        result: &result,
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
