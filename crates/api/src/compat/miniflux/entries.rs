use super::error::{from_api_error, internal, not_found, MfResult};
use super::types::{map_feed, MfEnclosureDto, MfEntryDto, MfEntryResultSet};
use crate::auth::mf_auth;
use crate::entry_options::{apply_entry_flags, EntryUpdateFlags};
use crate::AppState;
use axum::extract::{Path, Query, State};
use axum::Json;
use chrono::{FixedOffset, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect, RelationTrait, Set,
};

use axum::response::IntoResponse;
use captura_pipeline::extractor;
use captura_storage::entity::{enclosure, entry, entry_label, feed, label};

#[derive(serde::Deserialize, Default)]
pub(crate) struct MfEntriesQuery {
    pub status: Option<String>,
    pub feed_id: Option<i64>,
    pub category_id: Option<i64>,
    pub starred: Option<bool>,
    pub search: Option<String>,
    pub before_id: Option<i64>,
    pub after_id: Option<i64>,
    #[serde(rename = "before_entry_id")]
    pub before_entry_id: Option<i64>,
    #[serde(rename = "after_entry_id")]
    pub after_entry_id: Option<i64>,
    pub order: Option<String>,     // published_at | id
    pub direction: Option<String>, // asc | desc
    pub content: Option<bool>,     // include content_html when true (default true)
    pub limit: Option<u64>,
    pub offset: Option<u64>,
    // time filters (epoch seconds)
    pub published_before: Option<i64>,
    pub published_after: Option<i64>,
    pub changed_before: Option<i64>,
    pub changed_after: Option<i64>,
}

pub(crate) async fn list(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Query(q): Query<MfEntriesQuery>,
) -> MfResult<Json<MfEntryResultSet>> {
    let auth = mf_auth(&st, &headers).await.map_err(from_api_error)?;
    // restrict to current user feeds
    let mut feed_sel = feed::Entity::find().filter(feed::Column::UserId.eq(auth.user_id));
    if let Some(cid) = q.category_id {
        feed_sel = feed_sel.filter(feed::Column::CategoryId.eq(cid));
    }
    let feed_ids: Vec<i64> = feed_sel
        .select_only()
        .column(feed::Column::Id)
        .into_tuple()
        .all(&st.db)
        .await
        .map_err(internal)?;
    let mut sel = entry::Entity::find().filter(entry::Column::FeedId.is_in(feed_ids));
    if let Some(fid) = q.feed_id {
        sel = sel.filter(entry::Column::FeedId.eq(fid));
    }
    if let Some(ref s) = q.status {
        match s.as_str() {
            "unread" => sel = sel.filter(entry::Column::IsRead.eq(false)),
            "read" => sel = sel.filter(entry::Column::IsRead.eq(true)),
            "starred" => sel = sel.filter(entry::Column::IsStarred.eq(true)),
            _ => {}
        }
    }
    if let Some(star) = q.starred {
        sel = sel.filter(entry::Column::IsStarred.eq(star));
    }
    if let Some(ref k) = q.search {
        let backend = st.db.get_database_backend();
        let pq = crate::search::parse_query(k);
        if crate::search::is_pg(backend) {
            if let Some(ref g) = pq.general {
                sel = sel.filter(crate::search::fts_filter_expr_pg(g));
            }
            for v in &pq.title {
                sel = sel.filter(crate::search::fts_field_expr_pg("title", v));
            }
            for v in &pq.author {
                sel = sel.filter(crate::search::fts_field_expr_pg("author", v));
            }
            for v in &pq.url {
                sel = sel.filter(crate::search::fts_field_expr_pg("url", v));
            }
            if !pq.tags.is_empty() {
                let mut tag_cond = Condition::any();
                for t in &pq.tags {
                    tag_cond = tag_cond.add(crate::search::tag_exists_expr_pg(t));
                }
                sel = sel.filter(tag_cond);
            }
        } else {
            if let Some(ref g) = pq.general {
                let like = format!("%{}%", g);
                let cond = Condition::any()
                    .add(entry::Column::Title.like(like.as_str()))
                    .add(entry::Column::Summary.like(like.as_str()))
                    .add(entry::Column::ContentHtml.like(like.as_str()));
                sel = sel.filter(cond);
            }
            for v in &pq.title {
                sel = sel.filter(entry::Column::Title.like(format!("%{}%", v)));
            }
            for v in &pq.author {
                sel = sel.filter(entry::Column::Author.like(format!("%{}%", v)));
            }
            for v in &pq.url {
                sel = sel.filter(entry::Column::Url.like(format!("%{}%", v)));
            }
            if !pq.tags.is_empty() {
                let mut tag_cond = Condition::any();
                for t in &pq.tags {
                    tag_cond = tag_cond.add(crate::search::tag_exists_expr_like(t));
                }
                sel = sel.filter(tag_cond);
            }
        }
    }
    // time window filters
    if let Some(ts) = q.published_before {
        if let Some(dt) = chrono::DateTime::from_timestamp(ts, 0) {
            sel = sel.filter(
                entry::Column::PublishedAt
                    .lte(dt.with_timezone(&FixedOffset::east_opt(0).unwrap())),
            );
        }
    }
    if let Some(ts) = q.published_after {
        if let Some(dt) = chrono::DateTime::from_timestamp(ts, 0) {
            sel = sel.filter(
                entry::Column::PublishedAt
                    .gte(dt.with_timezone(&FixedOffset::east_opt(0).unwrap())),
            );
        }
    }
    if let Some(ts) = q.changed_before {
        if let Some(dt) = chrono::DateTime::from_timestamp(ts, 0) {
            sel = sel.filter(
                entry::Column::UpdatedAt.lte(dt.with_timezone(&FixedOffset::east_opt(0).unwrap())),
            );
        }
    }
    if let Some(ts) = q.changed_after {
        if let Some(dt) = chrono::DateTime::from_timestamp(ts, 0) {
            sel = sel.filter(
                entry::Column::UpdatedAt.gte(dt.with_timezone(&FixedOffset::east_opt(0).unwrap())),
            );
        }
    }
    // id before/after filters (compatible with both before_id/before_entry_id and after_id/after_entry_id)
    let before = q.before_id.or(q.before_entry_id);
    let after = q.after_id.or(q.after_entry_id);
    if let Some(b) = before {
        sel = sel.filter(entry::Column::Id.lt(b));
    }
    if let Some(a) = after {
        sel = sel.filter(entry::Column::Id.gt(a));
    }

    match q.order.as_deref() {
        Some("id") => {
            sel = if matches!(q.direction.as_deref(), Some("asc")) {
                sel.order_by_asc(entry::Column::Id)
            } else {
                sel.order_by_desc(entry::Column::Id)
            };
        }
        _ => {
            sel = if matches!(q.direction.as_deref(), Some("asc")) {
                sel.order_by_asc(entry::Column::PublishedAt)
            } else {
                sel.order_by_desc(entry::Column::PublishedAt)
            };
            sel = sel.order_by_desc(entry::Column::CreatedAt);
        }
    }
    let limit = q.limit.unwrap_or(100).min(1000);
    let count = sel.clone().count(&st.db).await.map_err(internal)? as i64;
    let rows = sel
        .find_also_related(feed::Entity)
        .limit(limit)
        .offset(q.offset.unwrap_or(0))
        .all(&st.db)
        .await
        .map_err(internal)?;
    let entry_ids: Vec<i64> = rows.iter().map(|(e, _)| e.id).collect();
    let mut enc_map: std::collections::HashMap<i64, Vec<MfEnclosureDto>> =
        std::collections::HashMap::new();
    let mut tag_map: std::collections::HashMap<i64, Vec<String>> = std::collections::HashMap::new();
    if !entry_ids.is_empty() {
        let encs = enclosure::Entity::find()
            .filter(enclosure::Column::EntryId.is_in(entry_ids.clone()))
            .all(&st.db)
            .await
            .map_err(internal)?;
        for en in encs {
            let list = enc_map.entry(en.entry_id).or_default();
            list.push(MfEnclosureDto {
                id: en.id,
                url: en.url,
                mime_type: en.mime.clone().unwrap_or_default(),
                size: en.length.unwrap_or(0),
                media_progression: 0,
            });
        }
        let pairs: Vec<(i64, String)> = entry_label::Entity::find()
            .join(
                sea_orm::JoinType::InnerJoin,
                entry_label::Relation::Label.def(),
            )
            .filter(entry_label::Column::EntryId.is_in(entry_ids.clone()))
            .filter(label::Column::UserId.eq(auth.user_id))
            .select_only()
            .column(entry_label::Column::EntryId)
            .column(label::Column::Name)
            .into_tuple()
            .all(&st.db)
            .await
            .map_err(internal)?;
        for (eid, name) in pairs {
            tag_map.entry(eid).or_default().push(name);
        }
    }
    let include_content = q.content.unwrap_or(true);
    let wpm: usize = std::env::var("READ_SPEED_WPM")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(200)
        .max(50) as usize;
    let entries = rows
        .into_iter()
        .map(|(e, fopt)| {
            let status = if e.is_read { "read" } else { "unread" }.to_string();
            let feed_dto = fopt.map(|f| map_feed(f, None));
            let encs = enc_map.get(&e.id).cloned();
            let tags = tag_map.get(&e.id).cloned().unwrap_or_default();
            let reading_time = if include_content {
                let body = e
                    .content_html
                    .clone()
                    .or(e.summary.clone())
                    .unwrap_or_default();
                captura_common::reading_time_minutes_from_html(&body, wpm)
            } else {
                0
            };
            MfEntryDto {
                id: e.id,
                date: e.published_at.map(|d| d.to_rfc3339()),
                changed_at: Some(e.updated_at.to_rfc3339()),
                created_at: e.created_at.to_rfc3339(),
                feed: feed_dto,
                hash: e.hash,
                url: e.url,
                comments_url: None,
                title: e.title,
                status,
                content: if include_content {
                    e.content_html
                } else {
                    None
                },
                author: e.author,
                share_code: None,
                enclosures: encs,
                tags,
                reading_time,
                user_id: auth.user_id,
                feed_id: e.feed_id,
                starred: e.is_starred,
            }
        })
        .collect();
    Ok(Json(MfEntryResultSet {
        total: count,
        entries,
    }))
}

pub(crate) async fn get(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
) -> MfResult<Json<MfEntryDto>> {
    let auth = mf_auth(&st, &headers).await.map_err(from_api_error)?;
    let Some(e) = entry::Entity::find_by_id(id)
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("entry").into());
    };
    let Some(f) = feed::Entity::find_by_id(e.feed_id)
        .filter(feed::Column::UserId.eq(auth.user_id))
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("entry").into());
    };
    let pairs: Vec<(i64, String)> = entry_label::Entity::find()
        .join(
            sea_orm::JoinType::InnerJoin,
            entry_label::Relation::Label.def(),
        )
        .filter(entry_label::Column::EntryId.eq(id))
        .filter(label::Column::UserId.eq(auth.user_id))
        .select_only()
        .column(entry_label::Column::EntryId)
        .column(label::Column::Name)
        .into_tuple()
        .all(&st.db)
        .await
        .map_err(internal)?;
    let tags: Vec<String> = pairs.into_iter().map(|(_, n)| n).collect();
    let wpm: usize = std::env::var("READ_SPEED_WPM")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(200)
        .max(50) as usize;
    let body = e
        .content_html
        .clone()
        .or(e.summary.clone())
        .unwrap_or_default();
    let reading_time = captura_common::reading_time_minutes_from_html(&body, wpm);
    let dto = MfEntryDto {
        id: e.id,
        date: e.published_at.map(|d| d.to_rfc3339()),
        changed_at: Some(e.updated_at.to_rfc3339()),
        created_at: e.created_at.to_rfc3339(),
        feed: Some(map_feed(f, None)),
        hash: e.hash,
        url: e.url,
        comments_url: None,
        title: e.title,
        status: if e.is_read {
            "read".into()
        } else {
            "unread".into()
        },
        content: e.content_html,
        author: e.author,
        share_code: None,
        enclosures: None,
        tags,
        reading_time,
        user_id: auth.user_id,
        feed_id: e.feed_id,
        starred: e.is_starred,
    };
    Ok(Json(dto))
}

#[derive(serde::Deserialize)]
pub(crate) struct MfUpdateEntriesBulk {
    pub entry_ids: Vec<i64>,
    pub status: String,
}

pub(crate) async fn update_bulk(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(body): Json<MfUpdateEntriesBulk>,
) -> MfResult<axum::response::Response> {
    let auth = mf_auth(&st, &headers).await.map_err(from_api_error)?;
    if body.entry_ids.is_empty() {
        return Ok((
            axum::http::StatusCode::NO_CONTENT,
            axum::body::Body::empty(),
        )
            .into_response());
    }
    let feed_ids: Vec<i64> = feed::Entity::find()
        .filter(feed::Column::UserId.eq(auth.user_id))
        .select_only()
        .column(feed::Column::Id)
        .into_tuple()
        .all(&st.db)
        .await
        .map_err(internal)?;
    let mut upd = entry::Entity::update_many();
    match body.status.as_str() {
        "read" => {
            upd = upd.col_expr(entry::Column::IsRead, sea_orm::sea_query::Expr::value(true));
        }
        "unread" => {
            upd = upd.col_expr(
                entry::Column::IsRead,
                sea_orm::sea_query::Expr::value(false),
            );
        }
        _ => {}
    }
    upd = upd
        .filter(entry::Column::Id.is_in(body.entry_ids.clone()))
        .filter(entry::Column::FeedId.is_in(feed_ids));
    let _ = upd.exec(&st.db).await.map_err(internal)?;
    Ok((
        axum::http::StatusCode::NO_CONTENT,
        axum::body::Body::empty(),
    )
        .into_response())
}

#[derive(serde::Deserialize, Default)]
pub(crate) struct MfUpdateEntry {
    pub status: Option<String>,
    pub title: Option<String>,
    pub content: Option<String>,
}

pub(crate) async fn update(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<MfUpdateEntry>,
) -> MfResult<Json<MfEntryDto>> {
    let auth = mf_auth(&st, &headers).await.map_err(from_api_error)?;
    let Some(e) = entry::Entity::find_by_id(id)
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("entry").into());
    };
    let owned = feed::Entity::find_by_id(e.feed_id)
        .filter(feed::Column::UserId.eq(auth.user_id))
        .one(&st.db)
        .await
        .map_err(internal)?
        .is_some();
    if !owned {
        return Err(not_found("entry").into());
    }
    let mut am: entry::ActiveModel = e.into();
    if let Some(sts) = body.status.as_deref() {
        let flag = match sts {
            "read" => Some(true),
            "unread" => Some(false),
            _ => None,
        };
        if let Some(v) = flag {
            apply_entry_flags(
                &mut am,
                EntryUpdateFlags {
                    is_read: Some(v),
                    is_starred: None,
                },
            );
        }
    }
    if let Some(t) = body.title {
        am.title = Set(Some(t));
    }
    if let Some(c) = body.content {
        am.content_html = Set(Some(c));
    }
    let _ = am.update(&st.db).await.map_err(internal)?;
    // Return the updated entry (same shape as GET /v1/entries/:id)
    let Some(e) = entry::Entity::find_by_id(id)
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("entry").into());
    };
    let Some(f) = feed::Entity::find_by_id(e.feed_id)
        .filter(feed::Column::UserId.eq(auth.user_id))
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("entry").into());
    };
    let pairs: Vec<(i64, String)> = entry_label::Entity::find()
        .join(
            sea_orm::JoinType::InnerJoin,
            entry_label::Relation::Label.def(),
        )
        .filter(entry_label::Column::EntryId.eq(id))
        .filter(label::Column::UserId.eq(auth.user_id))
        .select_only()
        .column(entry_label::Column::EntryId)
        .column(label::Column::Name)
        .into_tuple()
        .all(&st.db)
        .await
        .map_err(internal)?;
    let tags: Vec<String> = pairs.into_iter().map(|(_, n)| n).collect();
    let wpm: usize = std::env::var("READ_SPEED_WPM")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(200)
        .max(50) as usize;
    let body_html = e
        .content_html
        .clone()
        .or(e.summary.clone())
        .unwrap_or_default();
    let reading_time = captura_common::reading_time_minutes_from_html(&body_html, wpm);
    let dto = MfEntryDto {
        id: e.id,
        date: e.published_at.map(|d| d.to_rfc3339()),
        changed_at: Some(e.updated_at.to_rfc3339()),
        created_at: e.created_at.to_rfc3339(),
        feed: Some(map_feed(f, None)),
        hash: e.hash,
        url: e.url,
        comments_url: None,
        title: e.title,
        status: if e.is_read {
            "read".into()
        } else {
            "unread".into()
        },
        content: e.content_html,
        author: e.author,
        share_code: None,
        enclosures: None,
        tags,
        reading_time,
        user_id: auth.user_id,
        feed_id: e.feed_id,
        starred: e.is_starred,
    };
    Ok(Json(dto))
}

pub(crate) async fn toggle_star(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
) -> MfResult<axum::response::Response> {
    let auth = mf_auth(&st, &headers).await.map_err(from_api_error)?;
    let Some(e) = entry::Entity::find_by_id(id)
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("entry").into());
    };
    let owned = feed::Entity::find_by_id(e.feed_id)
        .filter(feed::Column::UserId.eq(auth.user_id))
        .one(&st.db)
        .await
        .map_err(internal)?
        .is_some();
    if !owned {
        return Err(not_found("entry").into());
    }
    let current = e.is_starred;
    let mut am: entry::ActiveModel = e.into();
    apply_entry_flags(
        &mut am,
        EntryUpdateFlags {
            is_read: None,
            is_starred: Some(!current),
        },
    );
    am.update(&st.db).await.map_err(internal)?;
    Ok((
        axum::http::StatusCode::NO_CONTENT,
        axum::body::Body::empty(),
    )
        .into_response())
}

// Cleanup history: delete read, unstarred entries older than the threshold for the current user
pub(crate) async fn flush_history(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
) -> MfResult<axum::response::Response> {
    let auth = mf_auth(&st, &headers).await.map_err(from_api_error)?;
    let days: i64 = std::env::var("FLUSH_HISTORY_DAYS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(30)
        .max(1);
    let cutoff =
        Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap()) - chrono::Duration::days(days);
    let feed_ids: Vec<i64> = feed::Entity::find()
        .filter(feed::Column::UserId.eq(auth.user_id))
        .select_only()
        .column(feed::Column::Id)
        .into_tuple()
        .all(&st.db)
        .await
        .map_err(internal)?;
    if feed_ids.is_empty() {
        return Ok((axum::http::StatusCode::ACCEPTED, axum::body::Body::empty()).into_response());
    }
    let _ = entry::Entity::delete_many()
        .filter(entry::Column::FeedId.is_in(feed_ids))
        .filter(entry::Column::IsRead.eq(true))
        .filter(entry::Column::IsStarred.eq(false))
        .filter(entry::Column::UpdatedAt.lte(cutoff))
        .exec(&st.db)
        .await
        .map_err(internal)?;
    Ok((axum::http::StatusCode::ACCEPTED, axum::body::Body::empty()).into_response())
}

#[derive(serde::Serialize)]
pub(crate) struct MfEntryContentResp {
    pub content: String,
}

#[derive(serde::Deserialize, Default)]
pub(crate) struct MfFetchContentQuery {
    pub update_content: Option<bool>,
}

// GET /v1/entries/:id/fetch-content
pub(crate) async fn fetch_content(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
    Query(q): Query<MfFetchContentQuery>,
) -> MfResult<Json<MfEntryContentResp>> {
    let auth = mf_auth(&st, &headers).await.map_err(from_api_error)?;
    let Some(e) = entry::Entity::find_by_id(id)
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("entry").into());
    };
    let owned = feed::Entity::find_by_id(e.feed_id)
        .filter(feed::Column::UserId.eq(auth.user_id))
        .one(&st.db)
        .await
        .map_err(internal)?
        .is_some();
    if !owned {
        return Err(not_found("entry").into());
    }
    let Some(f) = feed::Entity::find_by_id(e.feed_id)
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("feed").into());
    };
    let page_url = match e.url.as_deref() {
        Some(u) => u,
        None => {
            let content = e
                .content_html
                .unwrap_or_else(|| e.summary.unwrap_or_default());
            return Ok(Json(MfEntryContentResp { content }));
        }
    };
    // Use the shared internal extraction service to fetch & extract full content
    let extracted = extractor::fetch_and_extract_entry(page_url, &f)
        .await
        .map_err(internal)?;
    let mut out_html = extracted.content_html.clone();
    let new_title = extracted.title;
    if out_html.is_empty() {
        // Stay compatible with the previous behavior: if extraction yields empty content, fall back to existing content/summary
        out_html = e
            .content_html
            .clone()
            .unwrap_or_else(|| e.summary.clone().unwrap_or_default());
    }
    // Optionally persist extracted content/title back to the database
    if q.update_content.unwrap_or(false) {
        if let Some(model) = entry::Entity::find_by_id(e.id)
            .one(&st.db)
            .await
            .map_err(internal)?
        {
            let mut am: entry::ActiveModel = model.into();
            am.content_html = Set(Some(out_html.clone()));
            if let Some(nt) = new_title {
                am.title = Set(Some(nt));
            }
            am.updated_at = Set(Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap()));
            let _ = am.update(&st.db).await.map_err(internal)?;
        }
    }
    Ok(Json(MfEntryContentResp { content: out_html }))
}

// POST /v1/entries/:id/save
pub(crate) async fn save(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
) -> MfResult<&'static str> {
    let auth = mf_auth(&st, &headers).await.map_err(from_api_error)?;
    let Some(e) = entry::Entity::find_by_id(id)
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("entry").into());
    };
    let owned = feed::Entity::find_by_id(e.feed_id)
        .filter(feed::Column::UserId.eq(auth.user_id))
        .one(&st.db)
        .await
        .map_err(internal)?
        .is_some();
    if !owned {
        return Err(not_found("entry").into());
    }
    let now = Utc::now()
        .with_timezone(&FixedOffset::east_opt(0).unwrap())
        .to_rfc3339();
    let extras = serde_json::json!({"saved": true, "saved_at": now});
    let mut am: entry::ActiveModel = e.into();
    am.extras_json = Set(Some(extras));
    let _ = am.update(&st.db).await.map_err(internal)?;
    if let Some(model) = entry::Entity::find_by_id(id)
        .one(&st.db)
        .await
        .map_err(internal)?
    {
        let _ = captura_service::webhook::emit_save_entry(
            &st.db,
            captura_common::UserId(auth.user_id),
            &model,
        )
        .await;
        let payload = captura_common::IntegrationEvent::SaveEntry {
            entry_id: model.id,
            feed_id: Some(model.feed_id),
        };
        let _ = captura_scheduler::enqueue_integration_event(
            &st.db,
            captura_common::UserId(auth.user_id),
            Some(model.feed_id),
            payload,
        )
        .await;
    }
    Ok("ok")
}

#[derive(serde::Deserialize)]
pub(crate) struct MfSetTagsReq {
    pub tags: Vec<String>,
}

// POST /v1/entries/:id/tags
pub(crate) async fn add_tags(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<MfSetTagsReq>,
) -> MfResult<&'static str> {
    let auth = mf_auth(&st, &headers).await.map_err(from_api_error)?;
    let Some(e) = entry::Entity::find_by_id(id)
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("entry").into());
    };
    let owned = feed::Entity::find_by_id(e.feed_id)
        .filter(feed::Column::UserId.eq(auth.user_id))
        .one(&st.db)
        .await
        .map_err(internal)?
        .is_some();
    if !owned {
        return Err(not_found("entry").into());
    }
    let mut names: Vec<String> = body
        .tags
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    names.sort();
    names.dedup();
    if names.is_empty() {
        return Ok("ok");
    }
    let existing: Vec<(i64, String)> = label::Entity::find()
        .filter(label::Column::UserId.eq(auth.user_id))
        .filter(label::Column::Name.is_in(names.clone()))
        .select_only()
        .column(label::Column::Id)
        .column(label::Column::Name)
        .into_tuple()
        .all(&st.db)
        .await
        .map_err(internal)?;
    let mut name_to_id: std::collections::HashMap<String, i64> =
        existing.into_iter().map(|(id, n)| (n, id)).collect();
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let missing: Vec<String> = names
        .iter()
        .filter(|n| !name_to_id.contains_key(*n))
        .cloned()
        .collect();
    for n in missing {
        let am = label::ActiveModel {
            user_id: Set(auth.user_id),
            name: Set(n.clone()),
            color: Set(None),
            created_at: Set(now),
            ..Default::default()
        };
        let l = am.insert(&st.db).await.map_err(internal)?;
        name_to_id.insert(n, l.id);
    }
    let label_ids: Vec<i64> = names
        .iter()
        .filter_map(|n| name_to_id.get(n).copied())
        .collect();
    if !label_ids.is_empty() {
        let existing_pairs: Vec<i64> = entry_label::Entity::find()
            .filter(entry_label::Column::EntryId.eq(id))
            .filter(entry_label::Column::LabelId.is_in(label_ids.clone()))
            .select_only()
            .column(entry_label::Column::LabelId)
            .into_tuple()
            .all(&st.db)
            .await
            .map_err(internal)?;
        let exist_set: std::collections::HashSet<i64> = existing_pairs.into_iter().collect();
        for lid in label_ids.into_iter().filter(|lid| !exist_set.contains(lid)) {
            let am = entry_label::ActiveModel {
                entry_id: Set(id),
                label_id: Set(lid),
                ..Default::default()
            };
            let _ = am.insert(&st.db).await.map_err(internal)?;
        }
    }
    Ok("ok")
}

// DELETE /v1/entries/:id/tags
pub(crate) async fn remove_tags(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<MfSetTagsReq>,
) -> MfResult<&'static str> {
    let auth = mf_auth(&st, &headers).await?;
    let Some(e) = entry::Entity::find_by_id(id)
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("entry").into());
    };
    let owned = feed::Entity::find_by_id(e.feed_id)
        .filter(feed::Column::UserId.eq(auth.user_id))
        .one(&st.db)
        .await
        .map_err(internal)?
        .is_some();
    if !owned {
        return Err(not_found("entry").into());
    }
    let mut names: Vec<String> = body
        .tags
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    names.sort();
    names.dedup();
    if names.is_empty() {
        return Ok("ok");
    }
    let label_ids: Vec<i64> = label::Entity::find()
        .filter(label::Column::UserId.eq(auth.user_id))
        .filter(label::Column::Name.is_in(names))
        .select_only()
        .column(label::Column::Id)
        .into_tuple()
        .all(&st.db)
        .await
        .map_err(internal)?;
    if !label_ids.is_empty() {
        let _ = entry_label::Entity::delete_many()
            .filter(entry_label::Column::EntryId.eq(id))
            .filter(entry_label::Column::LabelId.is_in(label_ids))
            .exec(&st.db)
            .await
            .map_err(internal)?;
    }
    Ok("ok")
}

// GET /v1/feeds/:id/entries -> wrapper
pub(crate) async fn feed_entries(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
    Query(mut q): Query<MfEntriesQuery>,
) -> MfResult<Json<MfEntryResultSet>> {
    q.feed_id = Some(id);
    list(State(st), headers, Query(q)).await
}
