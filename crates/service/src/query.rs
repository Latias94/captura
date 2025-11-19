//! Read-side queries and counters shared between API layers.
//!
//! This module centralizes common read operations so that both
//! /api/v1 (first-party API) and /v1 (Miniflux compatibility) can
//! reuse the same logic.

use captura_common::Result;
use captura_storage::entity::{entry, entry_label, feed, label};
use captura_types::EntryView;
use sea_orm::{
    ColumnTrait, Condition, DatabaseConnection, EntityTrait, JoinType, Order, QueryFilter,
    QueryOrder, QuerySelect, RelationTrait,
};
use std::collections::HashMap;

/// Compute read/unread counters per feed for a given user.
///
/// This mirrors the behaviour exposed by:
/// - /api/v1/feeds/counters (FeedCountersDto)
/// - /v1/feeds?withCounters=true (Miniflux counters)
pub async fn feed_counters_for_user(
    db: &DatabaseConnection,
    user_id: i64,
) -> Result<(HashMap<i64, i64>, HashMap<i64, i64>)> {
    let feed_ids: Vec<i64> = feed::Entity::find()
        .filter(feed::Column::UserId.eq(user_id))
        .select_only()
        .column(feed::Column::Id)
        .into_tuple()
        .all(db)
        .await
        .map_err(|e| captura_common::Error::Storage(e.to_string()))?;
    let mut reads: HashMap<i64, i64> = HashMap::new();
    let mut unreads: HashMap<i64, i64> = HashMap::new();
    if feed_ids.is_empty() {
        return Ok((reads, unreads));
    }

    // unread per feed
    let unread_pairs: Vec<(i64, i64)> = entry::Entity::find()
        .filter(entry::Column::FeedId.is_in(feed_ids.clone()))
        .filter(entry::Column::IsRead.eq(false))
        .select_only()
        .column(entry::Column::FeedId)
        .column_as(entry::Column::Id.count(), "cnt")
        .group_by(entry::Column::FeedId)
        .into_tuple()
        .all(db)
        .await
        .map_err(|e| captura_common::Error::Storage(e.to_string()))?;
    for (fid, cnt) in unread_pairs {
        unreads.insert(fid, cnt);
    }

    // read per feed
    let read_pairs: Vec<(i64, i64)> = entry::Entity::find()
        .filter(entry::Column::FeedId.is_in(feed_ids))
        .filter(entry::Column::IsRead.eq(true))
        .select_only()
        .column(entry::Column::FeedId)
        .column_as(entry::Column::Id.count(), "cnt")
        .group_by(entry::Column::FeedId)
        .into_tuple()
        .all(db)
        .await
        .map_err(|e| captura_common::Error::Storage(e.to_string()))?;
    for (fid, cnt) in read_pairs {
        reads.insert(fid, cnt);
    }

    Ok((reads, unreads))
}

/// Compute unread counters per category for a given user.
///
/// For /api/v1 this is returned as CategoryCounterDto, where
/// `category_id = None` represents "uncategorized".
pub async fn category_unread_counters_for_user(
    db: &DatabaseConnection,
    user_id: i64,
) -> Result<HashMap<Option<i64>, i64>> {
    let feeds = feed::Entity::find()
        .filter(feed::Column::UserId.eq(user_id))
        .all(db)
        .await
        .map_err(|e| captura_common::Error::Storage(e.to_string()))?;
    let feed_ids: Vec<i64> = feeds.iter().map(|f| f.id).collect();
    let mut cat_map: HashMap<Option<i64>, i64> = HashMap::new();
    if feed_ids.is_empty() {
        return Ok(cat_map);
    }
    let pairs: Vec<(i64, i64)> = entry::Entity::find()
        .filter(entry::Column::FeedId.is_in(feed_ids.clone()))
        .filter(entry::Column::IsRead.eq(false))
        .select_only()
        .column(entry::Column::FeedId)
        .column_as(entry::Column::Id.count(), "cnt")
        .group_by(entry::Column::FeedId)
        .into_tuple()
        .all(db)
        .await
        .map_err(|e| captura_common::Error::Storage(e.to_string()))?;
    let feed_cat: HashMap<i64, Option<i64>> =
        feeds.into_iter().map(|f| (f.id, f.category_id)).collect();
    for (fid, cnt) in pairs {
        let cat = feed_cat.get(&fid).cloned().unwrap_or(None);
        *cat_map.entry(cat).or_insert(0) += cnt;
    }
    Ok(cat_map)
}

/// Filter describing which entries should be selected for read/unread ops
/// or future list queries.
///
/// Currently supports feed/category/label dimensions, with `view` reserved
/// as the base view concept.
#[derive(Debug, Default, Clone)]
pub struct EntryQueryFilter {
    pub feed_id: Option<i64>,
    pub category_id: Option<i64>,
    pub label_ids: Vec<i64>,
    pub view: Option<EntryView>,
}

/// Build a view-based filter condition on `feed.view` for timeline-style queries.
///
/// Semantics:
/// - `None` or `Some(All)` → no additional condition;
/// - `Some(Articles)`      → `feed.view IS NULL OR feed.view = 'articles'`;
/// - other concrete views  → `feed.view = '<view>'` (exact match).
pub fn view_filter_condition(view: Option<EntryView>) -> Option<Condition> {
    let view = view?;
    if matches!(view, EntryView::All) {
        return None;
    }
    let view_str = view.as_str().to_string();
    if matches!(view, EntryView::Articles) {
        Some(
            Condition::any()
                .add(feed::Column::View.is_null())
                .add(feed::Column::View.eq(view_str)),
        )
    } else {
        Some(Condition::all().add(feed::Column::View.eq(view_str)))
    }
}

async fn mark_entries_read_with_filter(
    db: &DatabaseConnection,
    user_id: i64,
    filter: &EntryQueryFilter,
) -> Result<u64> {
    // Only process entries belonging to the current user (constrained via feed.user_id).
    let mut sel = entry::Entity::find()
        .join(JoinType::InnerJoin, entry::Relation::Feed.def())
        .filter(feed::Column::UserId.eq(user_id));

    if let Some(fid) = filter.feed_id {
        sel = sel.filter(entry::Column::FeedId.eq(fid));
    }
    if let Some(cid) = filter.category_id {
        sel = sel.filter(feed::Column::CategoryId.eq(cid));
    }
    if let Some(cond) = view_filter_condition(filter.view) {
        sel = sel.filter(cond);
    }
    // NOTE: label-based scoping for mark-all-read is handled separately
    // in `mark_entries_read_for_labels` to avoid complex joins here.

    let ids: Vec<i64> = sel
        .select_only()
        .column(entry::Column::Id)
        .into_tuple()
        .all(db)
        .await
        .map_err(|e| captura_common::Error::Storage(e.to_string()))?;
    if ids.is_empty() {
        return Ok(0);
    }

    let res = entry::Entity::update_many()
        .col_expr(entry::Column::IsRead, sea_orm::sea_query::Expr::value(true))
        .filter(entry::Column::Id.is_in(ids))
        .exec(db)
        .await
        .map_err(|e| captura_common::Error::Storage(e.to_string()))?;
    Ok(res.rows_affected as u64)
}

/// Mark entries as read for a given user, optionally scoped by feed/category/view.
///
/// Semantics:
/// - Always restricts to entries belonging to `user_id` via an inner join on feed;
/// - When `feed_id` is `Some`, only entries from that feed are affected;
/// - When `category_id` is `Some`, only entries whose feed is in that category are affected;
/// - When `view` is `Some` and not `All`, only entries from feeds with that view are affected;
/// - When all filters are `None`, all entries of the user are marked as read.
pub async fn mark_entries_read_for_user(
    db: &DatabaseConnection,
    user_id: i64,
    feed_id: Option<i64>,
    category_id: Option<i64>,
    view: Option<EntryView>,
) -> Result<u64> {
    let filter = EntryQueryFilter {
        feed_id,
        category_id,
        label_ids: Vec::new(),
        view,
    };
    mark_entries_read_with_filter(db, user_id, &filter).await
}

/// Mark entries as read by label (label/tag dimension).
///
/// Primarily used for the Miniflux compatibility layer's label-level
/// “mark all as read”, and can be extended with more complex grouping
/// logic in the future.
pub async fn mark_entries_read_for_labels(
    db: &DatabaseConnection,
    user_id: i64,
    label_ids: &[i64],
) -> Result<u64> {
    if label_ids.is_empty() {
        return Ok(0);
    }
    // Select entry ids for the given labels, scoped to the current user.
    let ids: Vec<i64> = entry_label::Entity::find()
        .join(JoinType::InnerJoin, entry_label::Relation::Entry.def())
        .join(JoinType::InnerJoin, entry_label::Relation::Label.def())
        .filter(label::Column::UserId.eq(user_id))
        .filter(entry_label::Column::LabelId.is_in(label_ids.to_vec()))
        .select_only()
        .column(entry::Column::Id)
        .into_tuple()
        .all(db)
        .await
        .map_err(|e| captura_common::Error::Storage(e.to_string()))?;
    if ids.is_empty() {
        return Ok(0);
    }
    let res = entry::Entity::update_many()
        .col_expr(entry::Column::IsRead, sea_orm::sea_query::Expr::value(true))
        .filter(entry::Column::Id.is_in(ids))
        .exec(db)
        .await
        .map_err(|e| captura_common::Error::Storage(e.to_string()))?;
    Ok(res.rows_affected as u64)
}

/// Logical status filter for timeline queries.
#[derive(Debug, Clone, Copy)]
pub enum TimelineStatus {
    Read,
    Unread,
    Starred,
}

/// Unified timeline query used by first-party APIs and SmartViews.
///
/// This captures the core dimensions used by Captura and Folo-style
/// timelines: view, feed/category/label subsets, status and search,
/// plus simple sort/paging parameters.
#[derive(Debug, Clone)]
pub struct TimelineQuery {
    pub view: Option<EntryView>,
    pub feed_ids: Vec<i64>,
    pub category_ids: Vec<i64>,
    pub label_ids: Vec<i64>,
    pub status: Option<TimelineStatus>,
    pub search: Option<String>,
    pub sort_by: Option<String>, // published_at | created_at | relevance
    pub sort_order: Option<String>, // asc | desc
    pub limit: u64,
    pub offset: u64,
    pub before_id: Option<i64>,
    pub after_id: Option<i64>,
}

/// List entries for a user according to a unified timeline query.
///
/// This function is intentionally view-aware and reuses the same
/// semantics across:
/// - `/api/v1/entries` (global timeline);
/// - `/api/v1/smart-views/{id}/entries` (named timelines);
/// - future timeline-style surfaces.
pub async fn list_entries_for_user(
    db: &DatabaseConnection,
    user_id: i64,
    q: &TimelineQuery,
) -> Result<Vec<entry::Model>> {
    let backend = db.get_database_backend();

    let mut sel = entry::Entity::find()
        .join(JoinType::InnerJoin, entry::Relation::Feed.def())
        .filter(feed::Column::UserId.eq(user_id));

    if !q.feed_ids.is_empty() {
        sel = sel.filter(entry::Column::FeedId.is_in(q.feed_ids.clone()));
    }
    if !q.category_ids.is_empty() {
        sel = sel.filter(feed::Column::CategoryId.is_in(q.category_ids.clone()));
    }
    if let Some(cond) = view_filter_condition(q.view) {
        sel = sel.filter(cond);
    }
    if !q.label_ids.is_empty() {
        // Join entry_label in reverse (entry has_many entry_label) so that we
        // can filter by label ids without introducing a second `entry` alias.
        sel = sel
            .join_rev(JoinType::InnerJoin, entry_label::Relation::Entry.def())
            .filter(entry_label::Column::LabelId.is_in(q.label_ids.clone()));
    }
    if let Some(status) = q.status {
        match status {
            TimelineStatus::Read => {
                sel = sel.filter(entry::Column::IsRead.eq(true));
            }
            TimelineStatus::Unread => {
                sel = sel.filter(entry::Column::IsRead.eq(false));
            }
            TimelineStatus::Starred => {
                sel = sel.filter(entry::Column::IsStarred.eq(true));
            }
        }
    }

    if let Some(b) = q.before_id {
        sel = sel.filter(entry::Column::Id.lt(b));
    }
    if let Some(a) = q.after_id {
        sel = sel.filter(entry::Column::Id.gt(a));
    }

    if let Some(ref search_str) = q.search {
        let pq = crate::search::parse_query(search_str);
        if crate::search::is_pg(backend) {
            if let Some(ref g) = pq.general {
                sel = sel.filter(crate::search::fts_filter_expr_pg(g));
                // Align with Miniflux/Folo-style UX: when searching and no
                // explicit sort_by is provided, default to relevance; only
                // use sort_by when explicitly requested.
                let want_rank = q
                    .sort_by
                    .as_deref()
                    .map(|s| s == "relevance")
                    .unwrap_or(true);
                if want_rank {
                    let ord = match q.sort_order.as_deref() {
                        Some("asc") => Order::Asc,
                        _ => Order::Desc,
                    };
                    sel = sel
                        .order_by(crate::search::fts_rank_expr_pg(g), ord)
                        .order_by_desc(entry::Column::PublishedAt)
                        .order_by_desc(entry::Column::CreatedAt);
                }
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
            // Non-Postgres fallback: LIKE matching
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

    match q.sort_by.as_deref() {
        Some("created_at") => {
            sel = match q.sort_order.as_deref() {
                Some("asc") => sel.order_by_asc(entry::Column::CreatedAt),
                _ => sel.order_by_desc(entry::Column::CreatedAt),
            };
        }
        Some("id") => {
            sel = match q.sort_order.as_deref() {
                Some("asc") => sel.order_by_asc(entry::Column::Id),
                _ => sel.order_by_desc(entry::Column::Id),
            };
        }
        // Default: published_at (desc) with created_at as tie-breaker.
        _ => {
            sel = match q.sort_order.as_deref() {
                Some("asc") => sel.order_by_asc(entry::Column::PublishedAt),
                _ => sel.order_by_desc(entry::Column::PublishedAt),
            };
            sel = sel.order_by_desc(entry::Column::CreatedAt);
        }
    }

    let limit = q.limit.max(1);
    sel = sel.limit(limit);
    if q.offset > 0 {
        sel = sel.offset(q.offset);
    }

    let list = sel
        .all(db)
        .await
        .map_err(|e| captura_common::Error::Storage(e.to_string()))?;
    Ok(list)
}
