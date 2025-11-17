use super::entries::MfEntriesQuery;
use super::error::{bad_request, from_api_error, internal, not_found, MfResult};
use super::types::{map_feed, MfCategoryDto, MfEntryResultSet, MfFeedDto};
use crate::auth::mf_auth;
use crate::AppState;
use axum::extract::{Path, Query, State};
use axum::Json;
use chrono::{FixedOffset, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect, Set,
};

use captura_storage::entity::{category, entry, feed};

#[derive(serde::Deserialize, Default)]
pub(crate) struct MfCatListQuery {
    pub counts: Option<bool>,
}

pub(crate) async fn list(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Query(q): Query<MfCatListQuery>,
) -> MfResult<Json<Vec<MfCategoryDto>>> {
    let auth = mf_auth(&st, &headers).await.map_err(from_api_error)?;
    let cats = category::Entity::find()
        .filter(category::Column::UserId.eq(auth.user_id))
        .order_by_asc(category::Column::Id)
        .all(&st.db)
        .await
        .map_err(internal)?;
    let want_counts = q.counts.unwrap_or(false);
    if !want_counts {
        let out: Vec<MfCategoryDto> = cats
            .into_iter()
            .map(|c| MfCategoryDto {
                id: c.id,
                title: c.name,
                hide_globally: false,
                feed_count: None,
                total_unread: None,
            })
            .collect();
        return Ok(Json(out));
    }

    // Compute feed_count and total_unread per category
    let cat_ids: Vec<i64> = cats.iter().map(|c| c.id).collect();
    // Fetch all feeds for this user (used to aggregate unread counts by category)
    let feeds = feed::Entity::find()
        .filter(feed::Column::UserId.eq(auth.user_id))
        .all(&st.db)
        .await
        .map_err(internal)?;
    use std::collections::HashMap;
    let mut feed_count_map: HashMap<i64, i64> = HashMap::new();
    for f in feeds.iter() {
        if let Some(cid) = f.category_id {
            if cat_ids.contains(&cid) {
                *feed_count_map.entry(cid).or_insert(0) += 1;
            }
        }
    }
    let feed_ids: Vec<i64> = feeds.iter().map(|f| f.id).collect();
    let mut unread_map: HashMap<i64, i64> = HashMap::new(); // category_id -> unread
    if !feed_ids.is_empty() {
        let unread_pairs: Vec<(i64, i64)> = entry::Entity::find()
            .filter(entry::Column::FeedId.is_in(feed_ids.clone()))
            .filter(entry::Column::IsRead.eq(false))
            .select_only()
            .column(entry::Column::FeedId)
            .column_as(entry::Column::Id.count(), "cnt")
            .group_by(entry::Column::FeedId)
            .into_tuple()
            .all(&st.db)
            .await
            .map_err(internal)?;
        let feed_cat: HashMap<i64, Option<i64>> =
            feeds.into_iter().map(|f| (f.id, f.category_id)).collect();
        for (fid, cnt) in unread_pairs {
            if let Some(Some(cid)) = feed_cat.get(&fid) {
                if cat_ids.contains(cid) {
                    *unread_map.entry(*cid).or_insert(0) += cnt;
                }
            }
        }
    }
    let out: Vec<MfCategoryDto> = cats
        .into_iter()
        .map(|c| MfCategoryDto {
            id: c.id,
            title: c.name,
            hide_globally: false,
            feed_count: Some(*feed_count_map.get(&c.id).unwrap_or(&0)),
            total_unread: Some(*unread_map.get(&c.id).unwrap_or(&0)),
        })
        .collect();
    Ok(Json(out))
}

#[derive(serde::Deserialize)]
pub(crate) struct MfCreateCategory {
    pub title: String,
}

pub(crate) async fn create(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(body): Json<MfCreateCategory>,
) -> MfResult<Json<MfCategoryDto>> {
    let auth = mf_auth(&st, &headers).await.map_err(from_api_error)?;
    if body.title.trim().is_empty() {
        return Err(bad_request("title required").into());
    }
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let am = category::ActiveModel {
        user_id: Set(auth.user_id),
        name: Set(body.title.clone()),
        created_at: Set(now),
        ..Default::default()
    };
    let c = am.insert(&st.db).await.map_err(internal)?;
    Ok(Json(MfCategoryDto {
        id: c.id,
        title: c.name,
        hide_globally: false,
        feed_count: None,
        total_unread: None,
    }))
}

#[derive(serde::Serialize)]
pub(crate) struct MfCatCounter {
    pub category_id: Option<i64>,
    pub unread: i64,
}

pub(crate) async fn counters(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
) -> MfResult<Json<Vec<MfCatCounter>>> {
    let auth = mf_auth(&st, &headers).await.map_err(from_api_error)?;
    let cat_map =
        captura_service::query::category_unread_counters_for_user(&st.db, auth.user_id)
            .await
            .map_err(internal)?;
    let out = cat_map
        .into_iter()
        .map(|(category_id, unread)| MfCatCounter {
            category_id,
            unread,
        })
        .collect();
    Ok(Json(out))
}

pub(crate) async fn entries(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
    Query(mut q): Query<MfEntriesQuery>,
) -> MfResult<Json<MfEntryResultSet>> {
    q.category_id = Some(id);
    super::entries::list(State(st), headers, Query(q)).await
}

pub(crate) async fn feeds(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
) -> MfResult<Json<Vec<MfFeedDto>>> {
    let auth = mf_auth(&st, &headers).await.map_err(from_api_error)?;
    let list = feed::Entity::find()
        .filter(feed::Column::UserId.eq(auth.user_id))
        .filter(feed::Column::CategoryId.eq(id))
        .find_also_related(category::Entity)
        .all(&st.db)
        .await
        .map_err(internal)?;
    Ok(Json(
        list.into_iter().map(|(f, c)| map_feed(f, c)).collect(),
    ))
}

pub(crate) async fn refresh(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
) -> MfResult<&'static str> {
    let auth = mf_auth(&st, &headers).await.map_err(from_api_error)?;
    let feeds = feed::Entity::find()
        .filter(feed::Column::UserId.eq(auth.user_id))
        .filter(feed::Column::CategoryId.eq(id))
        .all(&st.db)
        .await
        .map_err(internal)?;
    for f in feeds {
        let _ = captura_service::refresh_and_persist(&st.db, &f).await;
    }
    Ok("ok")
}

#[derive(serde::Deserialize)]
pub(crate) struct MfUpdateCategory {
    pub title: String,
}

pub(crate) async fn update(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<MfUpdateCategory>,
) -> MfResult<&'static str> {
    let auth = mf_auth(&st, &headers).await.map_err(from_api_error)?;
    if body.title.trim().is_empty() {
        return Err(bad_request("title required").into());
    }
    let Some(cat) = category::Entity::find_by_id(id)
        .filter(category::Column::UserId.eq(auth.user_id))
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("category").into());
    };
    let mut am: category::ActiveModel = cat.into();
    am.name = Set(body.title);
    let _ = am.update(&st.db).await.map_err(internal)?;
    Ok("ok")
}

pub(crate) async fn delete(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
) -> MfResult<&'static str> {
    let auth = mf_auth(&st, &headers).await?;
    let _ = feed::Entity::update_many()
        .col_expr(
            feed::Column::CategoryId,
            sea_orm::sea_query::Expr::value(Option::<i64>::None),
        )
        .filter(feed::Column::UserId.eq(auth.user_id))
        .filter(feed::Column::CategoryId.eq(id))
        .exec(&st.db)
        .await
        .map_err(internal)?;
    if let Some(cat) = category::Entity::find_by_id(id)
        .filter(category::Column::UserId.eq(auth.user_id))
        .one(&st.db)
        .await
        .map_err(internal)?
    {
        let am: category::ActiveModel = cat.into();
        let _ = am.delete(&st.db).await.map_err(internal)?;
    }
    Ok("ok")
}

pub(crate) async fn mark_all_read(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
) -> MfResult<&'static str> {
    let auth = mf_auth(&st, &headers).await?;
    let _ = captura_service::query::mark_entries_read_for_user(
        &st.db,
        auth.user_id,
        None,
        Some(id),
        None,
    )
    .await
    .map_err(internal)?;
    Ok("ok")
}
