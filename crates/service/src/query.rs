//! Read-side queries and counters shared between API layers.
//!
//! This module centralizes common read operations so that both
//! /api/v1 (first-party API) and /v1 (Miniflux compatibility) can
//! reuse the same logic.

use captura_common::Result;
use captura_storage::entity::{entry, entry_label, feed};
use captura_types::EntryView;
use sea_orm::{
    ColumnTrait, Condition, DatabaseConnection, EntityTrait, JoinType, QueryFilter, QuerySelect,
    RelationTrait,
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
/// 目前支持 feed/category/label 维度，并预留 view 作为基础视图概念。
#[derive(Debug, Default, Clone)]
pub struct EntryQueryFilter {
    pub feed_id: Option<i64>,
    pub category_id: Option<i64>,
    pub label_ids: Vec<i64>,
    pub view: Option<EntryView>,
}

async fn mark_entries_read_with_filter(
    db: &DatabaseConnection,
    user_id: i64,
    filter: &EntryQueryFilter,
) -> Result<u64> {
    // 只处理当前用户的条目（通过 feed.user_id 约束）
    let mut sel = entry::Entity::find()
        .join(JoinType::InnerJoin, entry::Relation::Feed.def())
        .filter(feed::Column::UserId.eq(user_id));

    if let Some(fid) = filter.feed_id {
        sel = sel.filter(entry::Column::FeedId.eq(fid));
    }
    if let Some(cid) = filter.category_id {
        sel = sel.filter(feed::Column::CategoryId.eq(cid));
    }
    if let Some(view) = filter.view {
        if !matches!(view, EntryView::All) {
            // feed.view is stored as snake_case string; when unset, it is treated as the default view ("articles").
            // For view=Articles we match both NULL and explicit "articles"; other views match by exact value.
            let view_str = view.as_str().to_string();
            if matches!(view, EntryView::Articles) {
                let cond = Condition::any()
                    .add(feed::Column::View.is_null())
                    .add(feed::Column::View.eq(view_str));
                sel = sel.filter(cond);
            } else {
                sel = sel.filter(feed::Column::View.eq(view_str));
            }
        }
    }
    if !filter.label_ids.is_empty() {
        sel = sel
            .join(JoinType::InnerJoin, entry_label::Relation::Entry.def())
            .filter(entry_label::Column::LabelId.is_in(filter.label_ids.clone()));
    }

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

/// 按标签（label/tag）维度将条目标记为已读。
///
/// 目前主要用于 Miniflux 兼容层的标签级“全部已读”，
/// 后续可以在此之上构建更复杂的分组逻辑。
pub async fn mark_entries_read_for_labels(
    db: &DatabaseConnection,
    user_id: i64,
    label_ids: &[i64],
) -> Result<u64> {
    if label_ids.is_empty() {
        return Ok(0);
    }
    let filter = EntryQueryFilter {
        feed_id: None,
        category_id: None,
        label_ids: label_ids.to_vec(),
        view: None,
    };
    mark_entries_read_with_filter(db, user_id, &filter).await
}
