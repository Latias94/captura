use super::error::{bad_request, from_api_error, internal, not_found, MfResult};
use crate::auth::mf_auth;
use crate::AppState;
use axum::extract::{Query, State};
use axum::Json;
use reqwest::Client;
use scraper::{Html, Selector};
use std::collections::HashSet;
use url::Url;

#[derive(serde::Serialize, Clone)]
pub(crate) struct MfSubscriptionDto {
    pub url: String,
    pub title: String,
    #[serde(rename = "type")]
    pub typ: String,
}

#[derive(serde::Deserialize, Default)]
pub(crate) struct MfDiscoverQuery {
    pub verify: Option<bool>,
}

#[derive(serde::Deserialize)]
pub(crate) struct MfDiscoverReq {
    pub url: String,
}

pub(crate) async fn discover(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Query(q): Query<MfDiscoverQuery>,
    Json(body): Json<MfDiscoverReq>,
) -> MfResult<Json<Vec<MfSubscriptionDto>>> {
    let _auth = mf_auth(&st, &headers).await.map_err(from_api_error)?;
    let base = Url::parse(&body.url).map_err(|_| bad_request("invalid url"))?;
    let client = Client::builder()
        .user_agent("captura-discover/0.1")
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .map_err(internal)?;
    let resp = client.get(base.clone()).send().await.map_err(internal)?;
    if !resp.status().is_success() {
        return Err(not_found("unreachable").into());
    }
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_lowercase();
    let body_text = resp.text().await.map_err(internal)?;
    let lower = body_text[..std::cmp::min(body_text.len(), 8192)].to_ascii_lowercase();
    let mut list: Vec<MfSubscriptionDto> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    fn push_candidate(
        list: &mut Vec<MfSubscriptionDto>,
        seen: &mut HashSet<String>,
        title: String,
        url: String,
        typ: &str,
    ) {
        if seen.insert(url.clone()) {
            list.push(MfSubscriptionDto {
                url,
                title,
                typ: typ.into(),
            });
        }
    }
    if content_type.contains("xml") || lower.contains("<rss") || lower.contains("<feed") {
        push_candidate(
            &mut list,
            &mut seen,
            base.as_str().to_string(),
            base.as_str().to_string(),
            if lower.contains("<rss") {
                "rss"
            } else {
                "atom"
            },
        );
    }
    if content_type.contains("json") || lower.contains("jsonfeed.org/version") {
        push_candidate(
            &mut list,
            &mut seen,
            base.as_str().to_string(),
            base.as_str().to_string(),
            "json",
        );
    }
    if content_type.contains("html") || (!content_type.contains("xml") && lower.contains("<html")) {
        let doc = Html::parse_document(&body_text);
        let sel = Selector::parse("link[rel]").unwrap();
        for el in doc.select(&sel) {
            let rel = el.value().attr("rel").unwrap_or("").to_ascii_lowercase();
            if !rel.contains("alternate") {
                continue;
            }
            let typ = el.value().attr("type").unwrap_or("").to_ascii_lowercase();
            let href = match el.value().attr("href") {
                Some(h) if !h.is_empty() => h,
                _ => continue,
            };
            let Ok(abs) = base.join(href) else { continue };
            let title = el.value().attr("title").unwrap_or("");
            if typ.contains("rss") || typ.contains("xml") {
                push_candidate(
                    &mut list,
                    &mut seen,
                    title.to_string().if_empty(abs.as_str()),
                    abs.to_string(),
                    "rss",
                );
            } else if typ.contains("atom") {
                push_candidate(
                    &mut list,
                    &mut seen,
                    title.to_string().if_empty(abs.as_str()),
                    abs.to_string(),
                    "atom",
                );
            } else if typ.contains("json") {
                push_candidate(
                    &mut list,
                    &mut seen,
                    title.to_string().if_empty(abs.as_str()),
                    abs.to_string(),
                    "json",
                );
            } else if typ.is_empty() {
                let href_l = abs.as_str().to_ascii_lowercase();
                let guess = if href_l.ends_with(".xml")
                    || href_l.contains("/feed")
                    || href_l.contains("/rss")
                {
                    Some("rss")
                } else if href_l.ends_with(".atom") || href_l.contains("/atom") {
                    Some("atom")
                } else if href_l.ends_with(".json") {
                    Some("json")
                } else {
                    None
                };
                if let Some(t) = guess {
                    push_candidate(
                        &mut list,
                        &mut seen,
                        title.to_string().if_empty(abs.as_str()),
                        abs.to_string(),
                        t,
                    );
                }
            }
        }
        if list.is_empty() {
            for suffix in [
                "/feed",
                "/feed.xml",
                "/rss",
                "/index.xml",
                "/atom.xml",
                "/feed.json",
            ] {
                if let Ok(abs) = base.join(suffix) {
                    let href_l = abs.as_str().to_ascii_lowercase();
                    let t = if href_l.ends_with(".json") {
                        "json"
                    } else if href_l.contains("atom") {
                        "atom"
                    } else {
                        "rss"
                    };
                    push_candidate(
                        &mut list,
                        &mut seen,
                        abs.as_str().to_string(),
                        abs.as_str().to_string(),
                        t,
                    );
                }
            }
        }
    }
    if list.is_empty() {
        return Err(not_found("no_subscription").into());
    }
    if q.verify.unwrap_or(false) {
        let mut verified: Vec<MfSubscriptionDto> = Vec::new();
        for cand in list.into_iter().take(10) {
            let resp = client.head(&cand.url).send().await;
            let ok = matches!(resp, Ok(r) if r.status().is_success())
                || matches!(
                    client.get(&cand.url).send().await,
                    Ok(r) if r.status().is_success()
                );
            if ok {
                verified.push(cand);
            }
        }
        if verified.is_empty() {
            return Err(not_found("no_subscription").into());
        }
        return Ok(Json(verified));
    }
    Ok(Json(list))
}

trait IfEmpty {
    fn if_empty(self, fallback: &str) -> String;
}
impl IfEmpty for String {
    fn if_empty(self, fallback: &str) -> String {
        if self.trim().is_empty() {
            fallback.to_string()
        } else {
            self
        }
    }
}
