use axum::{
    extract::{Path, Query, State},
    Json,
};
use axum_extra::typed_header::TypedHeader;
use headers::authorization::Bearer;
use headers::Authorization;
use sea_orm::QuerySelect;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, EntityTrait, QueryFilter, QueryOrder, RelationTrait,
    Set,
};
use serde::{Deserialize, Serialize};

use captura_storage::entity::{entry, feed, prelude::*};

use crate::auth::AuthUser;
use crate::error::{bad_request, internal, ApiResult};
use crate::AppState;
use crate::{validate_limit_offset, validate_sort};

#[derive(Deserialize)]
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
    pub limit: Option<u64>,
    pub offset: Option<u64>,
    pub q: Option<String>,
    pub sort_by: Option<String>,
    pub order: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct EntryDto {
    pub id: i64,
    pub feed_id: i64,
    pub url: Option<String>,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub content_html: Option<String>,
    pub author: Option<String>,
    pub published_at: Option<String>,
    pub is_read: bool,
    pub is_starred: bool,
}

pub(crate) async fn list_entries(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Query(q): Query<EntriesQuery>,
) -> ApiResult<Json<Vec<EntryDto>>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    validate_limit_offset(q.limit, q.offset)?;
    validate_sort(&q.sort_by, &["published_at", "created_at"], &q.order)?;
    if let Some(ref s) = q.q {
        if s.len() > 256 {
            return Err(bad_request("q too long"));
        }
    }
    let sel0 = Entry::find();
    let mut sel = sea_orm::QuerySelect::join(
        sel0,
        sea_orm::JoinType::InnerJoin,
        entry::Relation::Feed.def(),
    )
    .filter(feed::Column::UserId.eq(user.user_id));
    if let Some(fid) = q.feed_id {
        sel = sel.filter(entry::Column::FeedId.eq(fid));
    }
    if let Some(cid) = q.category_id {
        sel = sel.filter(feed::Column::CategoryId.eq(cid));
    }
    if let Some(sts) = &q.status {
        match sts {
            StatusFilter::Read => sel = sel.filter(entry::Column::IsRead.eq(true)),
            StatusFilter::Unread => sel = sel.filter(entry::Column::IsRead.eq(false)),
            StatusFilter::Starred => sel = sel.filter(entry::Column::IsStarred.eq(true)),
        }
    }
    if let Some(ref qstr) = q.q {
        let like = format!("%{}%", qstr);
        let cond = Condition::any()
            .add(entry::Column::Title.like(like.as_str()))
            .add(entry::Column::Summary.like(like.as_str()))
            .add(entry::Column::ContentHtml.like(like.as_str()));
        sel = sel.filter(cond);
    }
    match q.sort_by.as_deref() {
        Some("created_at") => {
            sel = match q.order.as_deref() {
                Some("asc") => sel.order_by_asc(entry::Column::CreatedAt),
                _ => sel.order_by_desc(entry::Column::CreatedAt),
            };
        }
        _ => {
            sel = match q.order.as_deref() {
                Some("asc") => sel.order_by_asc(entry::Column::PublishedAt),
                _ => sel.order_by_desc(entry::Column::PublishedAt),
            };
            sel = sel.order_by_desc(entry::Column::CreatedAt);
        }
    }
    let l = q.limit.unwrap_or(100);
    sel = sea_orm::QuerySelect::limit(sel, l);
    if let Some(o) = q.offset {
        sel = sea_orm::QuerySelect::offset(sel, o);
    }
    let list = sel.all(&st.db).await.map_err(internal)?;
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
    if let Some(e) = Entry::find_by_id(id).one(&st.db).await.map_err(internal)? {
        let owned = Feed::find_by_id(e.feed_id)
            .filter(feed::Column::UserId.eq(user.user_id))
            .one(&st.db)
            .await
            .map_err(internal)?
            .is_some();
        if !owned {
            return Err(crate::error::forbidden("not your entry"));
        }
        let mut am: entry::ActiveModel = e.into();
        am.is_read = Set(body.value);
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
    if let Some(e) = Entry::find_by_id(id).one(&st.db).await.map_err(internal)? {
        let owned = Feed::find_by_id(e.feed_id)
            .filter(feed::Column::UserId.eq(user.user_id))
            .one(&st.db)
            .await
            .map_err(internal)?
            .is_some();
        if !owned {
            return Err(crate::error::forbidden("not your entry"));
        }
        let mut am: entry::ActiveModel = e.into();
        am.is_starred = Set(body.value);
        am.update(&st.db).await.map_err(internal)?;
    }
    Ok("ok")
}

#[derive(Deserialize)]
pub(crate) struct MarkAllReq {
    pub feed_id: Option<i64>,
    pub category_id: Option<i64>,
}

pub(crate) async fn mark_all_read(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Json(body): Json<MarkAllReq>,
) -> ApiResult<&'static str> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    if body.feed_id.is_none() && body.category_id.is_none() {
        return Err(bad_request("feed_id or category_id required"));
    }
    let sel0 = Entry::find();
    let mut sel = sea_orm::QuerySelect::join(
        sel0,
        sea_orm::JoinType::InnerJoin,
        entry::Relation::Feed.def(),
    )
    .filter(feed::Column::UserId.eq(user.user_id));
    if let Some(fid) = body.feed_id {
        sel = sel.filter(entry::Column::FeedId.eq(fid));
    }
    if let Some(cid) = body.category_id {
        sel = sel.filter(feed::Column::CategoryId.eq(cid));
    }
    let ids: Vec<i64> = sel
        .select_only()
        .column(entry::Column::Id)
        .into_tuple()
        .all(&st.db)
        .await
        .map_err(internal)?;
    if !ids.is_empty() {
        entry::Entity::update_many()
            .col_expr(entry::Column::IsRead, sea_orm::sea_query::Expr::value(true))
            .filter(entry::Column::Id.is_in(ids))
            .exec(&st.db)
            .await
            .map_err(internal)?;
    }
    Ok("ok")
}
