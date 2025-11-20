use super::error::{from_api_error, internal, not_found, MfResult};
use super::types::{map_feed, MfEnclosureDto, MfEntryDto, MfEntryResultSet};
use crate::auth::mf_auth;
use crate::entry_options::{apply_entry_flags, EntryUpdateFlags};
use crate::AppState;
use axum::extract::{Path, Query, State};
use axum::Json;
use chrono::{FixedOffset, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QuerySelect,
    RelationTrait, Set,
};

use axum::response::IntoResponse;
use captura_service::query::{build_timeline_select, TimelineQuery, TimelineStatus};
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
    // Map Miniflux query parameters onto the unified timeline model.
    let mut feed_ids = Vec::new();
    if let Some(fid) = q.feed_id {
        feed_ids.push(fid);
    }
    let mut category_ids = Vec::new();
    if let Some(cid) = q.category_id {
        category_ids.push(cid);
    }
    let mut status = match q.status.as_deref() {
        Some("unread") => Some(TimelineStatus::Unread),
        Some("read") => Some(TimelineStatus::Read),
        Some("starred") => Some(TimelineStatus::Starred),
        _ => None,
    };
    // starred=true should behave as a shortcut for "starred entries".
    if q.starred == Some(true) {
        status = Some(TimelineStatus::Starred);
    }
    let before = q.before_id.or(q.before_entry_id);
    let after = q.after_id.or(q.after_entry_id);
    let sort_by = match q.order.as_deref() {
        Some("id") => Some("id".to_string()),
        _ => Some("published_at".to_string()),
    };
    let sort_order = q.direction.clone();
    let limit = q.limit.unwrap_or(100).min(1000);
    let offset = q.offset.unwrap_or(0);
    let mut tquery = TimelineQuery::new(
        Some(captura_types::EntryView::All),
        feed_ids,
        category_ids,
        Vec::new(),
        status,
        q.search.clone(),
        sort_by,
        sort_order,
        limit,
        offset,
        before,
        after,
    );
    // Map time window filters into the unified timeline query (epoch seconds → DateTime).
    if let Some(ts) = q.published_before {
        if let Some(dt) = chrono::DateTime::from_timestamp(ts, 0) {
            tquery.published_before = Some(dt.with_timezone(&FixedOffset::east_opt(0).unwrap()));
        }
    }
    if let Some(ts) = q.published_after {
        if let Some(dt) = chrono::DateTime::from_timestamp(ts, 0) {
            tquery.published_after = Some(dt.with_timezone(&FixedOffset::east_opt(0).unwrap()));
        }
    }
    if let Some(ts) = q.changed_before {
        if let Some(dt) = chrono::DateTime::from_timestamp(ts, 0) {
            tquery.changed_before = Some(dt.with_timezone(&FixedOffset::east_opt(0).unwrap()));
        }
    }
    if let Some(ts) = q.changed_after {
        if let Some(dt) = chrono::DateTime::from_timestamp(ts, 0) {
            tquery.changed_after = Some(dt.with_timezone(&FixedOffset::east_opt(0).unwrap()));
        }
    }
    let backend = st.db.get_database_backend();
    let mut sel = build_timeline_select(backend, auth.user_id, &tquery);
    // Optional starred=false filter (timeline already covered starred=true via status).
    if q.starred == Some(false) {
        sel = sel.filter(entry::Column::IsStarred.eq(false));
    }
    let count = sel.clone().count(&st.db).await.map_err(internal)? as i64;
    let rows: Vec<entry::Model> = sel
        .limit(limit)
        .offset(q.offset.unwrap_or(0))
        .all(&st.db)
        .await
        .map_err(internal)?;
    let entry_ids: Vec<i64> = rows.iter().map(|e| e.id).collect();
    use std::collections::HashMap;
    let mut enc_map: HashMap<i64, Vec<MfEnclosureDto>> = std::collections::HashMap::new();
    let mut tag_map: HashMap<i64, Vec<String>> = std::collections::HashMap::new();
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
    // Preload feeds for mapping to MfEntryDto.
    let mut feed_map: HashMap<i64, feed::Model> = HashMap::new();
    if !rows.is_empty() {
        let feed_ids: Vec<i64> = rows.iter().map(|e| e.feed_id).collect();
        let feeds = feed::Entity::find()
            .filter(feed::Column::Id.is_in(feed_ids.clone()))
            .all(&st.db)
            .await
            .map_err(internal)?;
        for f in feeds {
            feed_map.insert(f.id, f);
        }
    }

    let entries = rows
        .into_iter()
        .map(|e| {
            let status = if e.is_read { "read" } else { "unread" }.to_string();
            let feed_dto = feed_map.get(&e.feed_id).map(|f| map_feed(f.clone(), None));
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
        return Err(not_found("entry"));
    };
    let Some(f) = feed::Entity::find_by_id(e.feed_id)
        .filter(feed::Column::UserId.eq(auth.user_id))
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("entry"));
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
        return Err(not_found("entry"));
    };
    let owned = feed::Entity::find_by_id(e.feed_id)
        .filter(feed::Column::UserId.eq(auth.user_id))
        .one(&st.db)
        .await
        .map_err(internal)?
        .is_some();
    if !owned {
        return Err(not_found("entry"));
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
        return Err(not_found("entry"));
    };
    let Some(f) = feed::Entity::find_by_id(e.feed_id)
        .filter(feed::Column::UserId.eq(auth.user_id))
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("entry"));
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
        return Err(not_found("entry"));
    };
    let owned = feed::Entity::find_by_id(e.feed_id)
        .filter(feed::Column::UserId.eq(auth.user_id))
        .one(&st.db)
        .await
        .map_err(internal)?
        .is_some();
    if !owned {
        return Err(not_found("entry"));
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
        return Err(not_found("entry"));
    };
    let owned = feed::Entity::find_by_id(e.feed_id)
        .filter(feed::Column::UserId.eq(auth.user_id))
        .one(&st.db)
        .await
        .map_err(internal)?
        .is_some();
    if !owned {
        return Err(not_found("entry"));
    }
    let Some(f) = feed::Entity::find_by_id(e.feed_id)
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("feed"));
    };
    let dto =
        captura_service::entries::get_entry_content(&st.db, &e, q.update_content.unwrap_or(false))
            .await
            .map_err(internal)?;
    Ok(Json(MfEntryContentResp {
        content: dto.content_html,
    }))
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
        return Err(not_found("entry"));
    };
    let owned = feed::Entity::find_by_id(e.feed_id)
        .filter(feed::Column::UserId.eq(auth.user_id))
        .one(&st.db)
        .await
        .map_err(internal)?
        .is_some();
    if !owned {
        return Err(not_found("entry"));
    }
    let updated = captura_service::entries::set_entry_saved(&st.db, &e, true)
        .await
        .map_err(internal)?;
    let _ = captura_service::webhook::emit_save_entry(
        &st.db,
        captura_common::UserId(auth.user_id),
        &updated,
    )
    .await;
    let payload = captura_common::IntegrationEvent::SaveEntry {
        entry_id: updated.id,
        feed_id: Some(updated.feed_id),
    };
    let _ = captura_scheduler::enqueue_integration_event(
        &st.db,
        captura_common::UserId(auth.user_id),
        Some(updated.feed_id),
        payload,
    )
    .await;
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
        return Err(not_found("entry"));
    };
    let owned = feed::Entity::find_by_id(e.feed_id)
        .filter(feed::Column::UserId.eq(auth.user_id))
        .one(&st.db)
        .await
        .map_err(internal)?
        .is_some();
    if !owned {
        return Err(not_found("entry"));
    }
    captura_service::entries::add_tags_to_entry(
        &st.db,
        captura_common::UserId(auth.user_id),
        &e,
        body.tags,
    )
    .await
    .map_err(internal)?;
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
        return Err(not_found("entry"));
    };
    let owned = feed::Entity::find_by_id(e.feed_id)
        .filter(feed::Column::UserId.eq(auth.user_id))
        .one(&st.db)
        .await
        .map_err(internal)?
        .is_some();
    if !owned {
        return Err(not_found("entry"));
    }
    captura_service::entries::remove_tags_from_entry(
        &st.db,
        captura_common::UserId(auth.user_id),
        &e,
        body.tags,
    )
    .await
    .map_err(internal)?;
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
