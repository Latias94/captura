use axum::{Json, extract::State};
use axum_extra::typed_header::TypedHeader;
use chrono::{FixedOffset, Utc};
use headers::Authorization;
use headers::authorization::Bearer;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

use crate::AppState;
use crate::auth::AuthUser;
use crate::error::{ApiResult, bad_request, internal};
use captura_storage::entity::{category, feed, label, smart_view, user_pref};
use captura_types::{
    EntryView, ExportCategory, ExportFeed, ExportFeedFetch, ExportFeedFilters, ExportLabel,
    ExportSmartView, ExportUserPref, FullExport,
};
use sea_orm::{DatabaseTransaction, TransactionTrait};
use serde::Serialize;

pub(crate) async fn export(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
) -> ApiResult<(axum::http::HeaderMap, String)> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let cats = category::Entity::find()
        .filter(category::Column::UserId.eq(user.user_id))
        .all(&st.db)
        .await
        .map_err(internal)?;
    let feeds = feed::Entity::find()
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

/// Captura-native, view-aware JSON export: categories + feeds + smart views.
pub(crate) async fn export_full(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
) -> ApiResult<axum::Json<FullExport>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());

    let cats = category::Entity::find()
        .filter(category::Column::UserId.eq(user.user_id))
        .all(&st.db)
        .await
        .map_err(internal)?;
    let feeds = feed::Entity::find()
        .filter(feed::Column::UserId.eq(user.user_id))
        .all(&st.db)
        .await
        .map_err(internal)?;
    let labels = label::Entity::find()
        .filter(label::Column::UserId.eq(user.user_id))
        .all(&st.db)
        .await
        .map_err(internal)?;
    let prefs = user_pref::Entity::find()
        .filter(user_pref::Column::UserId.eq(user.user_id))
        .all(&st.db)
        .await
        .map_err(internal)?;
    let smart_views = smart_view::Entity::find()
        .filter(smart_view::Column::UserId.eq(user.user_id))
        .all(&st.db)
        .await
        .map_err(internal)?;
    let categories: Vec<ExportCategory> = cats
        .into_iter()
        .map(|c| ExportCategory {
            id: c.id,
            name: c.name,
            view: EntryView::from_db(c.view.as_deref()).unwrap_or(EntryView::Articles),
        })
        .collect();

    let feeds_out: Vec<ExportFeed> = feeds
        .into_iter()
        .map(|f| ExportFeed {
            id: f.id,
            title: f.title,
            site_url: f.site_url,
            feed_url: f.feed_url,
            category_id: f.category_id,
            view: EntryView::from_db(f.view.as_deref()).unwrap_or(EntryView::Articles),
            r#type: match f.r#type {
                feed::FeedType::Rss => "rss".to_string(),
                feed::FeedType::Atom => "atom".to_string(),
                feed::FeedType::Json => "json".to_string(),
                feed::FeedType::Rule => "rule".to_string(),
                feed::FeedType::Hub => "hub".to_string(),
            },
            fetch: ExportFeedFetch {
                user_agent: f.user_agent,
                headers_json: f.headers_json.and_then(|v| serde_json::to_value(v).ok()),
                cookies: f.cookies,
                proxy_url: f.proxy_url,
                fetch_via_proxy: f.fetch_via_proxy,
                disable_http2: f.disable_http2,
                allow_invalid_certs: f.allow_invalid_certs,
                request_timeout_ms: f.request_timeout_ms,
            },
            filters: ExportFeedFilters {
                scraper_rules: f.scraper_rules,
                rewrite_rules: f.rewrite_rules,
                blocklist_rules: f.blocklist_rules,
                keeplist_rules: f.keeplist_rules,
                url_rewrite_rules: f.url_rewrite_rules,
                block_filter_entry_rules: f.block_filter_entry_rules,
                keep_filter_entry_rules: f.keep_filter_entry_rules,
            },
        })
        .collect();

    let smart_out: Vec<ExportSmartView> = smart_views
        .into_iter()
        .map(|sv| ExportSmartView {
            id: sv.id,
            name: sv.name,
            view: EntryView::from_str(&sv.view).unwrap_or(EntryView::Articles),
            filters: sv.filters_json.unwrap_or(serde_json::json!({})),
            sort_by: sv.sort_by,
            sort_order: sv.sort_order,
            pinned: sv.pinned,
        })
        .collect();

    let labels_out: Vec<ExportLabel> = labels
        .into_iter()
        .map(|l| ExportLabel {
            id: l.id,
            name: l.name,
            color: l.color,
        })
        .collect();

    let user_prefs_out: Vec<ExportUserPref> = prefs
        .into_iter()
        .map(|p| ExportUserPref {
            key: p.key,
            value: p.value_json.and_then(|v| serde_json::to_value(v).ok()),
        })
        .collect();

    let export = FullExport {
        version: "1".to_string(),
        exported_at: now.to_rfc3339(),
        categories,
        feeds: feeds_out,
        smart_views: smart_out,
        labels: labels_out,
        user_prefs: user_prefs_out,
    };

    Ok(axum::Json(export))
}

/// Import a full JSON export produced by `/api/v1/export/full`.
///
/// Semantics (per user):
/// - categories: matched by name; existing categories are updated (view),
///   missing ones are created; their original `id` is only used to remap
///   feeds/smart_views in this payload;
/// - feeds: matched by `feed_url`; existing feeds are updated (category,
///   view, fetch + filter config); missing ones are created;
/// - smart_views: always created new for the current user; any `feed_ids` or
///   `category_ids` in the filters payload are remapped using the mappings
///   built from categories/feeds above (unknown ids are dropped).
pub(crate) async fn import_full(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Json(payload): Json<FullExport>,
) -> ApiResult<&'static str> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;

    // For now we only support version "1" (or empty/missing).
    if !payload.version.is_empty() && payload.version != "1" {
        return Err(bad_request("unsupported export version"));
    }

    // Basic safety limits to avoid accidentally importing extremely large snapshots.
    const MAX_CATEGORIES: usize = 5000;
    const MAX_FEEDS: usize = 20000;
    const MAX_SMART_VIEWS: usize = 5000;
    if payload.categories.len() > MAX_CATEGORIES {
        return Err(bad_request("too many categories in import payload"));
    }
    if payload.feeds.len() > MAX_FEEDS {
        return Err(bad_request("too many feeds in import payload"));
    }
    if payload.smart_views.len() > MAX_SMART_VIEWS {
        return Err(bad_request("too many smart views in import payload"));
    }

    // Run the import in a single DB transaction so we either fully apply the
    // snapshot or roll it back on any error.
    let txn = st.db.begin().await.map_err(internal)?;
    let res = import_full_inner(&txn, user.user_id, payload).await;
    match res {
        Ok(()) => {
            txn.commit().await.map_err(internal)?;
            Ok("ok")
        }
        Err(e) => {
            let _ = txn.rollback().await;
            Err(e)
        }
    }
}

async fn import_full_inner(
    db: &DatabaseTransaction,
    user_id: i64,
    payload: FullExport,
) -> ApiResult<()> {
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());

    // 0) Labels: name-based upsert, build old_id -> new_id map (for future
    // label_id remapping in filters). For now this mainly ensures labels are
    // recreated on the target instance.
    let mut label_id_map: std::collections::HashMap<i64, i64> = std::collections::HashMap::new();
    for l in &payload.labels {
        let existing = label::Entity::find()
            .filter(label::Column::UserId.eq(user_id))
            .filter(label::Column::Name.eq(l.name.clone()))
            .one(db)
            .await
            .map_err(internal)?;
        let new_id = if let Some(existing_label) = existing {
            let mut am: label::ActiveModel = existing_label.into();
            am.color = Set(l.color.clone());
            let updated = am.update(db).await.map_err(internal)?;
            updated.id
        } else {
            let am = label::ActiveModel {
                id: Default::default(),
                user_id: Set(user_id),
                name: Set(l.name.clone()),
                color: Set(l.color.clone()),
                created_at: Set(now),
            };
            let created = am.insert(db).await.map_err(internal)?;
            created.id
        };
        label_id_map.insert(l.id, new_id);
    }

    // 1) Categories: name-based upsert, build old_id -> new_id map.
    let mut cat_id_map: std::collections::HashMap<i64, i64> = std::collections::HashMap::new();
    for c in &payload.categories {
        // Find by (user_id, name).
        let existing = category::Entity::find()
            .filter(category::Column::UserId.eq(user_id))
            .filter(category::Column::Name.eq(c.name.clone()))
            .one(db)
            .await
            .map_err(internal)?;
        // Persisted categories never use the "all" logical view; treat it as
        // a request for the default articles view.
        let view_enum = match c.view {
            EntryView::All => EntryView::Articles,
            v => v,
        };
        let view_str = view_enum.to_db();
        let new_id = if let Some(existing_cat) = existing {
            // Update view to match import payload.
            let mut am: category::ActiveModel = existing_cat.into();
            am.view = Set(Some(view_str));
            let updated = am.update(db).await.map_err(internal)?;
            updated.id
        } else {
            let am = category::ActiveModel {
                user_id: Set(user_id),
                name: Set(c.name.clone()),
                view: Set(Some(view_str)),
                created_at: Set(now),
                ..Default::default()
            };
            let created = am.insert(db).await.map_err(internal)?;
            created.id
        };
        cat_id_map.insert(c.id, new_id);
    }

    // 1) Categories: name-based upsert, build old_id -> new_id map.
    // (this block already exists below)

    // 2) Feeds: feed_url-based upsert, build old_id -> new_id map.
    let mut feed_id_map: std::collections::HashMap<i64, i64> = std::collections::HashMap::new();
    for f in &payload.feeds {
        // Map category id via name-mapped categories above.
        let mapped_category_id = f.category_id.and_then(|old| cat_id_map.get(&old).copied());

        let existing = feed::Entity::find()
            .filter(feed::Column::UserId.eq(user_id))
            .filter(feed::Column::FeedUrl.eq(f.feed_url.clone()))
            .one(db)
            .await
            .map_err(internal)?;
        // Similar to categories, normalize "all" to the concrete articles view
        // before persisting.
        let view_enum = match f.view {
            EntryView::All => EntryView::Articles,
            v => v,
        };
        let view_str = view_enum.to_db();

        let new_id = if let Some(existing_feed) = existing {
            let mut am: feed::ActiveModel = existing_feed.into();
            am.category_id = Set(mapped_category_id);
            am.title = Set(f.title.clone());
            am.site_url = Set(f.site_url.clone());
            am.view = Set(Some(view_str));
            am.r#type = Set(match f.r#type.as_str() {
                "rss" => feed::FeedType::Rss,
                "atom" => feed::FeedType::Atom,
                "json" => feed::FeedType::Json,
                "rule" => feed::FeedType::Rule,
                "hub" => feed::FeedType::Hub,
                _ => feed::FeedType::Rss,
            });
            am.user_agent = Set(f.fetch.user_agent.clone());
            am.headers_json = Set(f.fetch.headers_json.clone());
            am.cookies = Set(f.fetch.cookies.clone());
            am.proxy_url = Set(f.fetch.proxy_url.clone());
            am.fetch_via_proxy = Set(f.fetch.fetch_via_proxy);
            am.disable_http2 = Set(f.fetch.disable_http2);
            am.allow_invalid_certs = Set(f.fetch.allow_invalid_certs);
            am.request_timeout_ms = Set(f.fetch.request_timeout_ms);
            am.scraper_rules = Set(f.filters.scraper_rules.clone());
            am.rewrite_rules = Set(f.filters.rewrite_rules.clone());
            am.blocklist_rules = Set(f.filters.blocklist_rules.clone());
            am.keeplist_rules = Set(f.filters.keeplist_rules.clone());
            am.url_rewrite_rules = Set(f.filters.url_rewrite_rules.clone());
            am.block_filter_entry_rules = Set(f.filters.block_filter_entry_rules.clone());
            am.keep_filter_entry_rules = Set(f.filters.keep_filter_entry_rules.clone());
            let updated = am.update(db).await.map_err(internal)?;
            updated.id
        } else {
            let am = feed::ActiveModel {
                id: Default::default(),
                user_id: Set(user_id),
                category_id: Set(mapped_category_id),
                r#type: Set(match f.r#type.as_str() {
                    "rss" => feed::FeedType::Rss,
                    "atom" => feed::FeedType::Atom,
                    "json" => feed::FeedType::Json,
                    "rule" => feed::FeedType::Rule,
                    "hub" => feed::FeedType::Hub,
                    _ => feed::FeedType::Rss,
                }),
                title: Set(f.title.clone()),
                site_url: Set(f.site_url.clone()),
                feed_url: Set(f.feed_url.clone()),
                favicon_id: Set(None),
                rule_id: Set(None),
                rule_params_json: Set(None),
                user_agent: Set(f.fetch.user_agent.clone()),
                username: Set(None),
                password: Set(None),
                headers_json: Set(f.fetch.headers_json.clone()),
                cookies: Set(f.fetch.cookies.clone()),
                proxy_url: Set(f.fetch.proxy_url.clone()),
                fetch_via_proxy: Set(f.fetch.fetch_via_proxy),
                disable_http2: Set(f.fetch.disable_http2),
                allow_invalid_certs: Set(f.fetch.allow_invalid_certs),
                request_timeout_ms: Set(f.fetch.request_timeout_ms),
                checked_at: Set(None),
                next_run_at: Set(None),
                etag: Set(None),
                last_modified: Set(None),
                last_status: Set(None),
                last_error_message: Set(None),
                error_count: Set(0),
                disabled: Set(false),
                view: Set(Some(view_str)),
                scraper_rules: Set(f.filters.scraper_rules.clone()),
                rewrite_rules: Set(f.filters.rewrite_rules.clone()),
                blocklist_rules: Set(f.filters.blocklist_rules.clone()),
                keeplist_rules: Set(f.filters.keeplist_rules.clone()),
                url_rewrite_rules: Set(f.filters.url_rewrite_rules.clone()),
                block_filter_entry_rules: Set(f.filters.block_filter_entry_rules.clone()),
                keep_filter_entry_rules: Set(f.filters.keep_filter_entry_rules.clone()),
                integrations_json: Set(None),
                created_at: Set(now),
                updated_at: Set(now),
            };
            let created = am.insert(db).await.map_err(internal)?;
            created.id
        };
        feed_id_map.insert(f.id, new_id);
    }

    // 3) Smart views: create new ones, remapping feed_ids/category_ids/label_ids when present.
    for sv in &payload.smart_views {
        let mut filters = sv.filters.clone();
        if let Some(obj) = filters.as_object_mut() {
            if let Some(feed_ids) = obj.get_mut("feed_ids") {
                if let Some(arr) = feed_ids.as_array_mut() {
                    let mut mapped: Vec<serde_json::Value> = Vec::new();
                    for id_val in arr.iter() {
                        if let Some(old_id) = id_val.as_i64() {
                            if let Some(new_id) = feed_id_map.get(&old_id) {
                                mapped.push(serde_json::json!(*new_id));
                            }
                        }
                    }
                    *arr = mapped;
                }
            }
            if let Some(cat_ids) = obj.get_mut("category_ids") {
                if let Some(arr) = cat_ids.as_array_mut() {
                    let mut mapped: Vec<serde_json::Value> = Vec::new();
                    for id_val in arr.iter() {
                        if let Some(old_id) = id_val.as_i64() {
                            if let Some(new_id) = cat_id_map.get(&old_id) {
                                mapped.push(serde_json::json!(*new_id));
                            }
                        }
                    }
                    *arr = mapped;
                }
            }
            if let Some(label_ids) = obj.get_mut("label_ids") {
                if let Some(arr) = label_ids.as_array_mut() {
                    let mut mapped: Vec<serde_json::Value> = Vec::new();
                    for id_val in arr.iter() {
                        if let Some(old_id) = id_val.as_i64() {
                            if let Some(new_id) = label_id_map.get(&old_id) {
                                mapped.push(serde_json::json!(*new_id));
                            }
                        }
                    }
                    *arr = mapped;
                }
            }
        }

        // Normalize the smart view's logical view, treating "all" as the
        // default articles view before persisting.
        let view_enum = match sv.view {
            EntryView::All => EntryView::Articles,
            v => v,
        };
        let view_str = view_enum.to_db();

        let am = smart_view::ActiveModel {
            id: Default::default(),
            user_id: Set(user_id),
            name: Set(sv.name.clone()),
            view: Set(view_str),
            filters_json: Set(Some(filters)),
            sort_by: Set(sv.sort_by.clone()),
            sort_order: Set(sv.sort_order.clone()),
            pinned: Set(sv.pinned),
            created_at: Set(now),
            updated_at: Set(now),
        };
        let _ = am.insert(db).await.map_err(internal)?;
    }

    Ok(())
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
    let mut cat_map: std::collections::HashMap<String, i64> = category::Entity::find()
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
                let dup = feed::Entity::find()
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

/// Response payload for OPML validation endpoint.
#[derive(Serialize)]
pub(crate) struct OpmlValidateResp {
    pub feeds: usize,
    pub categories: usize,
}

/// Validate an OPML document without mutating the database.
/// Returns counts of detected feeds and categories so the Web UI
/// can show a pre-import summary to the user.
pub(crate) async fn validate(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    body: String,
) -> ApiResult<axum::Json<OpmlValidateResp>> {
    // Require authentication for consistency with import/export endpoints.
    let _user = AuthUser::from_bearer(&st.db, bearer.token()).await?;

    const MAX_OPML_BYTES: usize = 2_000_000;
    if body.len() > MAX_OPML_BYTES {
        return Err(bad_request("OPML too large"));
    }

    // Try full XML parsing first; fall back to the tolerant extractor.
    let nodes = match parse_opml_quickxml(&body) {
        Ok(nodes) => nodes,
        Err(_) => extract_outlines(&body),
    };

    let mut feeds = 0usize;
    let mut categories = 0usize;
    for n in nodes {
        match n {
            OutlineNode::Feed { .. } => feeds += 1,
            OutlineNode::Category { .. } => categories += 1,
        }
    }

    Ok(axum::Json(OpmlValidateResp { feeds, categories }))
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
    // Simple fallback to tolerate common OPML structures
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
    use quick_xml::{Reader, events::Event};
    let mut reader = Reader::from_str(body);
    reader.config_mut().trim_text(true);
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
                        .decode_and_unescape_value(reader.decoder())
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
