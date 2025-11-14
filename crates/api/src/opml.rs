use axum::extract::State;
use axum_extra::typed_header::TypedHeader;
use chrono::{FixedOffset, Utc};
use headers::authorization::Bearer;
use headers::Authorization;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

use crate::auth::AuthUser;
use crate::error::{bad_request, internal, ApiResult};
use crate::AppState;
use captura_storage::entity::{category, feed, prelude::*};

pub(crate) async fn export(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
) -> ApiResult<(axum::http::HeaderMap, String)> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let cats = Category::find()
        .filter(category::Column::UserId.eq(user.user_id))
        .all(&st.db)
        .await
        .map_err(internal)?;
    let feeds = Feed::find()
        .filter(feed::Column::UserId.eq(user.user_id))
        .all(&st.db)
        .await
        .map_err(internal)?;
    let mut buf = String::new();
    buf.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<opml version=\"2.0\">\n<head><title>Captura Export</title></head>\n<body>\n");
    for f in feeds.iter().filter(|f| f.category_id.is_none()) {
        buf.push_str(&format!(
            "<outline text=\"{}\" title=\"{}\" type=\"rss\" xmlUrl=\"{}\" htmlUrl=\"{}\"/>\n",
            xml_escape(f.title.as_deref().unwrap_or("")),
            xml_escape(f.title.as_deref().unwrap_or("")),
            xml_escape(&f.feed_url),
            xml_escape(f.site_url.as_deref().unwrap_or(""))
        ));
    }
    for c in cats {
        buf.push_str(&format!(
            "<outline text=\"{}\" title=\"{}\">\n",
            xml_escape(&c.name),
            xml_escape(&c.name)
        ));
        for f in feeds.iter().filter(|f| f.category_id == Some(c.id)) {
            buf.push_str(&format!(
                "  <outline text=\"{}\" title=\"{}\" type=\"rss\" xmlUrl=\"{}\" htmlUrl=\"{}\"/>\n",
                xml_escape(f.title.as_deref().unwrap_or("")),
                xml_escape(f.title.as_deref().unwrap_or("")),
                xml_escape(&f.feed_url),
                xml_escape(f.site_url.as_deref().unwrap_or(""))
            ));
        }
        buf.push_str("</outline>\n");
    }
    buf.push_str("</body>\n</opml>\n");
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("application/xml; charset=utf-8"),
    );
    Ok((headers, buf))
}

pub(crate) async fn import(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    body: String,
) -> ApiResult<&'static str> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    const MAX_OPML_BYTES: usize = 2_000_000;
    if body.len() > MAX_OPML_BYTES {
        return Err(bad_request("OPML too large"));
    }
    let outlines = parse_opml_quickxml(&body).unwrap_or_else(|_| extract_outlines(&body));
    const MAX_OUTLINES: usize = 2000;
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let mut cat_map: std::collections::HashMap<String, i64> = Category::find()
        .filter(category::Column::UserId.eq(user.user_id))
        .all(&st.db)
        .await
        .map_err(internal)?
        .into_iter()
        .map(|c| (c.name.clone(), c.id))
        .collect();
    for node in outlines.into_iter().take(MAX_OUTLINES) {
        match node {
            OutlineNode::Feed {
                title,
                xml_url,
                html_url,
                category,
            } => {
                let category_id = if let Some(cat) = category {
                    if let Some(id) = cat_map.get(&cat).copied() {
                        Some(id)
                    } else {
                        let am = category::ActiveModel {
                            user_id: Set(user.user_id),
                            name: Set(cat.clone()),
                            created_at: Set(now),
                            ..Default::default()
                        };
                        let c = am.insert(&st.db).await.map_err(internal)?;
                        cat_map.insert(cat, c.id);
                        Some(c.id)
                    }
                } else {
                    None
                };
                let dup = Feed::find()
                    .filter(feed::Column::UserId.eq(user.user_id))
                    .filter(feed::Column::FeedUrl.eq(&xml_url))
                    .one(&st.db)
                    .await
                    .map_err(internal)?;
                if dup.is_some() {
                    continue;
                }
                let am = feed::ActiveModel {
                    user_id: Set(user.user_id),
                    category_id: Set(category_id),
                    r#type: Set(feed::FeedType::Rss),
                    title: Set(Some(title.unwrap_or_else(|| xml_url.clone()))),
                    site_url: Set(html_url),
                    feed_url: Set(xml_url),
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
            OutlineNode::Category { .. } => {}
        }
    }
    Ok("ok")
}

pub(crate) fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[derive(Debug)]
pub(crate) enum OutlineNode {
    Feed {
        title: Option<String>,
        xml_url: String,
        html_url: Option<String>,
        category: Option<String>,
    },
    Category {
        #[allow(dead_code)]
        title: String,
    },
}

pub(crate) fn extract_outlines(body: &str) -> Vec<OutlineNode> {
    // 朴素 fallback，容错解析常见 OPML 结构
    let mut nodes = Vec::new();
    for line in body.lines() {
        let line = line.trim();
        if line.contains("<outline") && line.contains("xmlUrl=") {
            let title = line
                .split("title=\"")
                .nth(1)
                .and_then(|s| s.split('"').next())
                .map(|s| s.to_string());
            let xml_url = line
                .split("xmlUrl=\"")
                .nth(1)
                .and_then(|s| s.split('"').next())
                .unwrap_or("")
                .to_string();
            let html_url = line
                .split("htmlUrl=\"")
                .nth(1)
                .and_then(|s| s.split('"').next())
                .map(|s| s.to_string());
            nodes.push(OutlineNode::Feed {
                title,
                xml_url,
                html_url,
                category: None,
            });
        } else if line.contains("<outline") && !line.contains("xmlUrl=") {
            let tt = line
                .split("title=\"")
                .nth(1)
                .and_then(|s| s.split('"').next())
                .unwrap_or("")
                .to_string();
            nodes.push(OutlineNode::Category { title: tt });
        } else if line.contains("</outline>") {
            // ignore
        }
    }
    nodes
}

pub(crate) fn parse_opml_quickxml(body: &str) -> Result<Vec<OutlineNode>, String> {
    use quick_xml::{events::Event, Reader};
    let mut reader = Reader::from_str(body);
    reader.trim_text(true);
    let mut buf = Vec::new();
    let mut nodes = Vec::new();
    let mut cat_stack: Vec<String> = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) if e.name().as_ref() == b"outline" => {
                let mut title = None;
                let mut text = None;
                let mut xml_url = None;
                let mut html_url = None;
                for a in e.attributes().with_checks(false).flatten() {
                    let key = a.key.as_ref();
                    let val = a
                        .decode_and_unescape_value(&reader)
                        .map_err(|e| e.to_string())?
                        .to_string();
                    match key {
                        b"title" => title = Some(val),
                        b"text" => text = Some(val),
                        b"xmlUrl" => xml_url = Some(val),
                        b"htmlUrl" => html_url = Some(val),
                        _ => {}
                    }
                }
                if let Some(xu) = xml_url {
                    let name = title.or(text);
                    let cat = cat_stack.last().cloned();
                    nodes.push(OutlineNode::Feed {
                        title: name,
                        xml_url: xu,
                        html_url,
                        category: cat,
                    });
                } else {
                    let name = title.or(text).unwrap_or_default();
                    nodes.push(OutlineNode::Category {
                        title: name.clone(),
                    });
                    cat_stack.push(name);
                }
            }
            Ok(Event::End(e)) => {
                if e.name().as_ref() == b"outline" {
                    let _ = cat_stack.pop();
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(e.to_string()),
            _ => {}
        }
    }
    Ok(nodes)
}
