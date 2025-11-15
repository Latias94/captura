use super::error::MfResult;
use crate::auth::mf_auth;
use crate::error::{bad_request, internal};
use crate::AppState;
use axum::extract::State;
// JSON helpers kept in error module; no direct Json import needed here
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, Set};

use captura_storage::entity::feed;

pub(crate) async fn export(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
) -> MfResult<(axum::http::HeaderMap, String)> {
    let auth = mf_auth(&st, &headers).await?;
    let feeds = feed::Entity::find()
        .filter(feed::Column::UserId.eq(auth.user_id))
        .all(&st.db)
        .await
        .map_err(internal)?;
    // Simplified exporter: minimal OPML (title/link) that is sufficient for UI/clients
    let mut body =
        String::from(r#"<?xml version="1.0" encoding="UTF-8"?><opml version="1.0"><body>"#);
    for f in feeds {
        body.push_str(&format!(
            "<outline text=\"{}\" xmlUrl=\"{}\"/>",
            f.title.clone().unwrap_or_else(|| f.feed_url.clone()),
            f.feed_url
        ));
    }
    body.push_str("</body></opml>");
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("text/xml; charset=utf-8"),
    );
    Ok((headers, body))
}

#[derive(serde::Deserialize)]
pub(crate) struct MfImportReq {
    pub content: String,
}

pub(crate) async fn import(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> MfResult<&'static str> {
    let auth = mf_auth(&st, &headers).await?;
    // Support Miniflux-style import: Content-Type: application/xml with raw OPML; keep backward compatibility with JSON {content}
    let content_type = headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    let body_str = String::from_utf8(body.to_vec()).unwrap_or_default();
    let xml_owned: String;
    let xml = if content_type.contains("xml") {
        xml_owned = body_str;
        xml_owned.trim()
    } else {
        // Backward-compatible JSON form
        if let Ok(req) = serde_json::from_str::<MfImportReq>(&body_str) {
            xml_owned = req.content;
            xml_owned.trim()
        } else {
            // Fallback: treat body as raw XML
            xml_owned = body_str;
            xml_owned.trim()
        }
    };
    if xml.is_empty() {
        return Err(bad_request("empty opml").into());
    }
    // Simplified importer: parse only xmlUrl attributes and create feeds if they do not yet exist
    let re = regex::Regex::new(r#"xmlUrl=\"([^\"]+)\""#).unwrap();
    let now = chrono::Utc::now().with_timezone(&chrono::FixedOffset::east_opt(0).unwrap());
    for cap in re.captures_iter(xml) {
        let url = cap
            .get(1)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        if url.is_empty() {
            continue;
        }
        let exists = feed::Entity::find()
            .filter(feed::Column::UserId.eq(auth.user_id))
            .filter(feed::Column::FeedUrl.eq(&url))
            .count(&st.db)
            .await
            .map_err(internal)?;
        if exists > 0 {
            continue;
        }
        let am = feed::ActiveModel {
            user_id: Set(auth.user_id),
            category_id: Set(None),
            r#type: Set(feed::FeedType::Rss),
            title: Set(None),
            site_url: Set(None),
            feed_url: Set(url),
            rule_id: Set(None),
            user_agent: Set(None),
            headers_json: Set(None),
            cookies: Set(None),
            proxy_url: Set(None),
            fetch_via_proxy: Set(false),
            disable_http2: Set(false),
            allow_invalid_certs: Set(false),
            request_timeout_ms: Set(None),
            checked_at: Set(None),
            next_run_at: Set(None),
            etag: Set(None),
            last_modified: Set(None),
            last_status: Set(None),
            error_count: Set(0),
            disabled: Set(false),
            scraper_rules: Set(None),
            rewrite_rules: Set(None),
            blocklist_rules: Set(None),
            keeplist_rules: Set(None),
            url_rewrite_rules: Set(None),
            block_filter_entry_rules: Set(None),
            keep_filter_entry_rules: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            favicon_id: Set(None),
            ..Default::default()
        };
        let _ = am.insert(&st.db).await.map_err(internal)?;
    }
    Ok("ok")
}
