use axum::{
    extract::{Path, Query, State},
    Json,
};
use axum_extra::typed_header::TypedHeader;
use chrono::FixedOffset;
use headers::authorization::Bearer;
use headers::Authorization;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QuerySelect,
    RelationTrait, Set, TransactionTrait,
};
use serde::Deserialize;

use crate::auth::AuthUser;
use crate::entry_options::{apply_entry_flags, EntryUpdateFlags};
use crate::error::{bad_request, internal, ApiResult};
use crate::util::{validate_limit_offset, validate_sort};
use crate::AppState;

use captura_pipeline::extractor;
use captura_service::query::{list_entries_for_user, TimelineQuery, TimelineStatus};
use captura_storage::entity::{entry, entry_label, feed, label};
use captura_types::{EntryContentDto, EntryDto, EntryView, Paging, Sorting};

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum StatusFilter {
    Read,
    Unread,
    Starred,
}

#[derive(Deserialize)]
pub(crate) struct EntriesQuery {
    pub feed_id: Option<i64>,
    pub category_id: Option<i64>,
    pub status: Option<StatusFilter>,
    #[serde(alias = "search")]
    pub q: Option<String>,
    pub view: Option<EntryView>,
    #[serde(flatten)]
    pub sorting: Sorting,
    #[serde(flatten)]
    pub paging: Paging,
    pub before_id: Option<i64>,
    pub after_id: Option<i64>,
    /// Optional flag: when true, preload tags for list entries.
    /// Defaults to false for performance reasons.
    pub include_tags: Option<bool>,
}

pub(crate) async fn list_entries(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Query(q): Query<EntriesQuery>,
) -> ApiResult<Json<Vec<EntryDto>>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    validate_limit_offset(q.paging.limit, q.paging.offset)?;
    validate_sort(
        &q.sorting.sort_by,
        &["published_at", "created_at", "relevance", "id"],
        &q.sorting.order,
    )?;
    if let Some(ref s) = q.q {
        if s.len() > 256 {
            return Err(bad_request("q too long"));
        }
    }
    let mut feed_ids = Vec::new();
    if let Some(fid) = q.feed_id {
        feed_ids.push(fid);
    }
    let mut category_ids = Vec::new();
    if let Some(cid) = q.category_id {
        category_ids.push(cid);
    }
    let status = q.status.map(|sts| match sts {
        StatusFilter::Read => TimelineStatus::Read,
        StatusFilter::Unread => TimelineStatus::Unread,
        StatusFilter::Starred => TimelineStatus::Starred,
    });
    let limit = q.paging.limit.unwrap_or(100);
    let offset = q.paging.offset.unwrap_or(0);
    let tquery = TimelineQuery {
        view: q.view,
        feed_ids,
        category_ids,
        label_ids: Vec::new(),
        status,
        search: q.q.clone(),
        sort_by: q.sorting.sort_by.clone(),
        sort_order: q.sorting.order.clone(),
        limit,
        offset,
        before_id: q.before_id,
        after_id: q.after_id,
    };
    let list = list_entries_for_user(&st.db, user.user_id, &tquery)
        .await
        .map_err(internal)?;
    // Optionally preload tags when requested.
    let mut tags_map: std::collections::HashMap<i64, Vec<String>> =
        std::collections::HashMap::new();
    if q.include_tags.unwrap_or(false) && !list.is_empty() {
        let ids: Vec<i64> = list.iter().map(|e| e.id).collect();
        let pairs: Vec<(i64, String)> = entry_label::Entity::find()
            .join(
                sea_orm::JoinType::InnerJoin,
                entry_label::Relation::Label.def(),
            )
            .filter(entry_label::Column::EntryId.is_in(ids))
            .filter(label::Column::UserId.eq(user.user_id))
            .select_only()
            .column(entry_label::Column::EntryId)
            .column(label::Column::Name)
            .into_tuple()
            .all(&st.db)
            .await
            .map_err(internal)?;
        for (eid, name) in pairs {
            tags_map.entry(eid).or_default().push(name);
        }
    }
    Ok(Json(
        list.into_iter()
            .map(|e| EntryDto {
                id: e.id,
                feed_id: e.feed_id,
                url: e.url,
                title: e.title,
                summary: e.summary,
                content_html: e.content_html,
                author: e.author,
                published_at: e.published_at.map(|d| d.to_rfc3339()),
                is_read: e.is_read,
                is_starred: e.is_starred,
                tags: tags_map.remove(&e.id),
            })
            .collect(),
    ))
}

#[derive(Deserialize)]
pub(crate) struct BoolBody {
    pub value: bool,
}

pub(crate) async fn mark_read(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(id): Path<i64>,
    Json(body): Json<BoolBody>,
) -> ApiResult<&'static str> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    if let Some(e) = load_owned_entry(&st.db, user.user_id, id).await? {
        let mut am: entry::ActiveModel = e.into();
        apply_entry_flags(
            &mut am,
            EntryUpdateFlags {
                is_read: Some(body.value),
                is_starred: None,
            },
        );
        am.update(&st.db).await.map_err(internal)?;
    }
    Ok("ok")
}

pub(crate) async fn mark_star(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(id): Path<i64>,
    Json(body): Json<BoolBody>,
) -> ApiResult<&'static str> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    if let Some(e) = load_owned_entry(&st.db, user.user_id, id).await? {
        let mut am: entry::ActiveModel = e.into();
        apply_entry_flags(
            &mut am,
            EntryUpdateFlags {
                is_read: None,
                is_starred: Some(body.value),
            },
        );
        am.update(&st.db).await.map_err(internal)?;
    }
    Ok("ok")
}

#[derive(Deserialize)]
pub(crate) struct EntriesBulkStatusReq {
    pub entry_ids: Vec<i64>,
    /// Supported values: "read" | "unread".
    pub status: String,
}

/// Bulk-update read status for a set of entries belonging to the current user.
///
/// Semantics:
/// - Only entries whose feed is owned by the authenticated user are affected;
/// - `status = "read"`   → `is_read = true`;
/// - `status = "unread"` → `is_read = false`;
/// - Other status values are rejected with 400.
pub(crate) async fn bulk_status(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Json(body): Json<EntriesBulkStatusReq>,
) -> ApiResult<&'static str> {
    use sea_orm::sea_query::Expr;

    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    if body.entry_ids.is_empty() {
        return Ok("ok");
    }
    let is_read = match body.status.as_str() {
        "read" => true,
        "unread" => false,
        _ => return Err(bad_request("unsupported status (expected read|unread)")),
    };
    // Restrict updates to entries whose feed is owned by this user.
    let feed_ids: Vec<i64> = feed::Entity::find()
        .filter(feed::Column::UserId.eq(user.user_id))
        .select_only()
        .column(feed::Column::Id)
        .into_tuple()
        .all(&st.db)
        .await
        .map_err(internal)?;
    if feed_ids.is_empty() {
        return Ok("ok");
    }
    let _ = entry::Entity::update_many()
        .col_expr(entry::Column::IsRead, Expr::value(is_read))
        .filter(entry::Column::Id.is_in(body.entry_ids.clone()))
        .filter(entry::Column::FeedId.is_in(feed_ids))
        .exec(&st.db)
        .await
        .map_err(internal)?;
    Ok("ok")
}

pub(crate) async fn get_entry(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(id): Path<i64>,
) -> ApiResult<Json<EntryDto>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let Some(e) = load_owned_entry(&st.db, user.user_id, id).await? else {
        return Err(crate::error::not_found("entry"));
    };
    // Load tag names for this entry for the current user.
    let pairs: Vec<(i64, String)> = entry_label::Entity::find()
        .join(
            sea_orm::JoinType::InnerJoin,
            entry_label::Relation::Label.def(),
        )
        .filter(entry_label::Column::EntryId.eq(id))
        .filter(label::Column::UserId.eq(user.user_id))
        .select_only()
        .column(entry_label::Column::EntryId)
        .column(label::Column::Name)
        .into_tuple()
        .all(&st.db)
        .await
        .map_err(internal)?;
    let tag_names: Vec<String> = pairs.into_iter().map(|(_, n)| n).collect();
    Ok(Json(EntryDto {
        id: e.id,
        feed_id: e.feed_id,
        url: e.url,
        title: e.title,
        summary: e.summary,
        content_html: e.content_html,
        author: e.author,
        published_at: e.published_at.map(|d| d.to_rfc3339()),
        is_read: e.is_read,
        is_starred: e.is_starred,
        tags: if tag_names.is_empty() {
            None
        } else {
            Some(tag_names)
        },
    }))
}

#[derive(Deserialize, Default)]
pub(crate) struct EntryContentQuery {
    pub update_content: Option<bool>,
}

pub(crate) async fn entry_content(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(id): Path<i64>,
    Query(q): Query<EntryContentQuery>,
) -> ApiResult<Json<EntryContentDto>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let Some(e) = load_owned_entry(&st.db, user.user_id, id).await? else {
        return Err(crate::error::not_found("entry"));
    };
    let Some(f) = feed::Entity::find_by_id(e.feed_id)
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(crate::error::not_found("feed"));
    };
    let page_url = match e.url.as_deref() {
        Some(u) => u,
        None => {
            let content = e
                .content_html
                .unwrap_or_else(|| e.summary.unwrap_or_default());
            return Ok(Json(EntryContentDto {
                content_html: content,
                title: e.title,
            }));
        }
    };
    let extracted = extractor::fetch_and_extract_entry(page_url, &f)
        .await
        .map_err(internal)?;
    let mut out_html = extracted.content_html.clone();
    let new_title = extracted.title;
    if out_html.is_empty() {
        let fallback = e
            .content_html
            .clone()
            .unwrap_or_else(|| e.summary.clone().unwrap_or_default());
        out_html = fallback;
    }
    if q.update_content.unwrap_or(false) {
        if let Some(model) = entry::Entity::find_by_id(e.id)
            .one(&st.db)
            .await
            .map_err(internal)?
        {
            let mut am: entry::ActiveModel = model.into();
            am.content_html = Set(Some(out_html.clone()));
            if let Some(nt) = new_title.clone() {
                am.title = Set(Some(nt));
            }
            am.updated_at =
                Set(chrono::Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap()));
            let _ = am.update(&st.db).await.map_err(internal)?;
        }
    }
    Ok(Json(EntryContentDto {
        content_html: out_html,
        title: new_title.or(e.title),
    }))
}

#[derive(Deserialize)]
pub(crate) struct SaveEntryReq {
    pub value: bool,
}

/// Mark an entry as "saved" (or clear the saved flag) for the current user.
///
/// When `value = true`:
/// - sets `entry.extras_json` to `{ "saved": true, "saved_at": "<rfc3339>" }`;
/// - emits a webhook event and enqueues an integration job, mirroring the
///   Miniflux-compatible `/v1/entries/:id/save` semantics.
///   When `value = false`:
/// - clears `entry.extras_json` for now (no webhook/integration side-effects).
pub(crate) async fn save_entry(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(id): Path<i64>,
    Json(body): Json<SaveEntryReq>,
) -> ApiResult<&'static str> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let Some(e) = load_owned_entry(&st.db, user.user_id, id).await? else {
        return Err(crate::error::not_found("entry"));
    };

    if body.value {
        let now = chrono::Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
        let saved_at = now.to_rfc3339();
        let extras = serde_json::json!({"saved": true, "saved_at": saved_at});
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
                captura_common::UserId(user.user_id),
                &model,
            )
            .await;
            let payload = captura_common::IntegrationEvent::SaveEntry {
                entry_id: model.id,
                feed_id: Some(model.feed_id),
            };
            let _ = captura_scheduler::enqueue_integration_event(
                &st.db,
                captura_common::UserId(user.user_id),
                Some(model.feed_id),
                payload,
            )
            .await;
        }
    } else {
        // For now we simply clear extras_json when value=false.
        let mut am: entry::ActiveModel = e.into();
        am.extras_json = Set(None);
        let _ = am.update(&st.db).await.map_err(internal)?;
    }

    Ok("ok")
}

#[derive(Deserialize)]
pub(crate) struct EntryTagsReq {
    pub tags: Vec<String>,
}

/// Add tags (labels) to an entry for the current user.
///
/// Semantics mirror the Miniflux-compatible `/v1/entries/:id/tags` endpoint:
/// - trims/normalizes tag names, deduplicates;
/// - creates missing labels for the user when necessary;
/// - creates `entry_label` relations when missing.
pub(crate) async fn add_tags(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(id): Path<i64>,
    Json(body): Json<EntryTagsReq>,
) -> ApiResult<&'static str> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let Some(e) = load_owned_entry(&st.db, user.user_id, id).await? else {
        return Err(crate::error::not_found("entry"));
    };
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
    // Run label creation and entry-label attachment in a single transaction
    // so we do not end up with partially created labels or relations.
    let txn = st.db.begin().await.map_err(internal)?;

    // Existing labels for this user (name -> id).
    let existing: Vec<(i64, String)> = label::Entity::find()
        .filter(label::Column::UserId.eq(user.user_id))
        .filter(label::Column::Name.is_in(names.clone()))
        .select_only()
        .column(label::Column::Id)
        .column(label::Column::Name)
        .into_tuple()
        .all(&txn)
        .await
        .map_err(internal)?;
    let mut name_to_id: std::collections::HashMap<String, i64> =
        existing.into_iter().map(|(id, n)| (n, id)).collect();

    // Create missing labels.
    let now = chrono::Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let missing: Vec<String> = names
        .iter()
        .filter(|n| !name_to_id.contains_key(*n))
        .cloned()
        .collect();
    for n in missing {
        let am = label::ActiveModel {
            id: Default::default(),
            user_id: Set(user.user_id),
            name: Set(n.clone()),
            color: Set(None),
            created_at: Set(now),
        };
        let l = am.insert(&txn).await.map_err(internal)?;
        name_to_id.insert(n, l.id);
    }

    // Attach labels to entry, avoiding duplicates.
    let label_ids: Vec<i64> = names
        .iter()
        .filter_map(|n| name_to_id.get(n).copied())
        .collect();
    if !label_ids.is_empty() {
        let existing_pairs: Vec<i64> = entry_label::Entity::find()
            .filter(entry_label::Column::EntryId.eq(e.id))
            .filter(entry_label::Column::LabelId.is_in(label_ids.clone()))
            .select_only()
            .column(entry_label::Column::LabelId)
            .into_tuple()
            .all(&txn)
            .await
            .map_err(internal)?;
        let exist_set: std::collections::HashSet<i64> = existing_pairs.into_iter().collect();
        for lid in label_ids.into_iter().filter(|lid| !exist_set.contains(lid)) {
            let am = entry_label::ActiveModel {
                entry_id: Set(e.id),
                label_id: Set(lid),
                ..Default::default()
            };
            let _ = am.insert(&txn).await.map_err(internal)?;
        }
    }

    txn.commit().await.map_err(internal)?;
    Ok("ok")
}

/// Remove tags (labels) from an entry for the current user.
pub(crate) async fn remove_tags(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(id): Path<i64>,
    Json(body): Json<EntryTagsReq>,
) -> ApiResult<&'static str> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let Some(e) = load_owned_entry(&st.db, user.user_id, id).await? else {
        return Err(crate::error::not_found("entry"));
    };
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

    // Remove tag relations in a transaction so we do not leave
    // partially removed state when errors occur mid-flight.
    let txn = st.db.begin().await.map_err(internal)?;

    let label_ids: Vec<i64> = label::Entity::find()
        .filter(label::Column::UserId.eq(user.user_id))
        .filter(label::Column::Name.is_in(names))
        .select_only()
        .column(label::Column::Id)
        .into_tuple()
        .all(&txn)
        .await
        .map_err(internal)?;
    if !label_ids.is_empty() {
        let _ = entry_label::Entity::delete_many()
            .filter(entry_label::Column::EntryId.eq(e.id))
            .filter(entry_label::Column::LabelId.is_in(label_ids))
            .exec(&txn)
            .await
            .map_err(internal)?;
    }
    txn.commit().await.map_err(internal)?;
    Ok("ok")
}

async fn load_owned_entry(
    db: &DatabaseConnection,
    user_id: i64,
    entry_id: i64,
) -> ApiResult<Option<entry::Model>> {
    if let Some(e) = entry::Entity::find_by_id(entry_id)
        .one(db)
        .await
        .map_err(internal)?
    {
        let owned = feed::Entity::find_by_id(e.feed_id)
            .filter(feed::Column::UserId.eq(user_id))
            .one(db)
            .await
            .map_err(internal)?
            .is_some();
        if !owned {
            return Err(crate::error::forbidden("not your entry"));
        }
        Ok(Some(e))
    } else {
        Ok(None)
    }
}

#[derive(Deserialize)]
pub(crate) struct MarkAllReq {
    pub feed_id: Option<i64>,
    pub category_id: Option<i64>,
    pub view: Option<EntryView>,
}

pub(crate) async fn mark_all_read(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Json(body): Json<MarkAllReq>,
) -> ApiResult<&'static str> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    if body.feed_id.is_none() && body.category_id.is_none() && body.view.is_none() {
        return Err(bad_request("feed_id, category_id or view required"));
    }
    let _ = captura_service::query::mark_entries_read_for_user(
        &st.db,
        user.user_id,
        body.feed_id,
        body.category_id,
        body.view,
    )
    .await
    .map_err(internal)?;
    Ok("ok")
}
