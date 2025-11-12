use super::error::MfResult;
use crate::auth::mf_auth;
use crate::error::{bad_request, internal};
use crate::AppState;
use axum::extract::State;
use axum::Json;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, Set};

use captura_storage::entity::feed;
use captura_storage::entity::prelude::*;

pub(crate) async fn export(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
) -> MfResult<(axum::http::HeaderMap, String)> {
    let auth = mf_auth(&st, &headers).await?;
    let feeds = Feed::find()
        .filter(feed::Column::UserId.eq(auth.user_id))
        .all(&st.db)
        .await
        .map_err(internal)?;
    // 简化：导出最小 OPML（标题/链接），UI/客户端可用
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
    Json(body): Json<MfImportReq>,
) -> MfResult<&'static str> {
    let auth = mf_auth(&st, &headers).await?;
    let xml = body.content.trim();
    if xml.is_empty() {
        return Err(bad_request("empty opml").into());
    }
    // 简化：仅解析 xmlUrl 属性，逐个创建 feed（如果不存在）
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
        let exists = Feed::find()
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
