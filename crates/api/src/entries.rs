use axum::{
    extract::{Path, Query, State},
    Json,
};
use axum_extra::typed_header::TypedHeader;
use headers::authorization::Bearer;
use headers::Authorization;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, DatabaseConnection, EntityTrait, JoinType, Order,
    QueryFilter, QueryOrder, QuerySelect, RelationTrait,
};
use serde::{Deserialize, Serialize};

use captura_storage::entity::{entry, feed};

use crate::auth::AuthUser;
use crate::entry_options::{apply_entry_flags, EntryUpdateFlags};
use crate::error::{bad_request, internal, ApiResult};
use crate::search;
use crate::util::{validate_limit_offset, validate_sort};
use crate::AppState;

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
    pub q: Option<String>,
    #[serde(flatten)]
    pub sorting: crate::util::Sorting,
    #[serde(flatten)]
    pub paging: crate::util::Paging,
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
    validate_limit_offset(q.paging.limit, q.paging.offset)?;
    validate_sort(
        &q.sorting.sort_by,
        &["published_at", "created_at", "relevance"],
        &q.sorting.order,
    )?;
    if let Some(ref s) = q.q {
        if s.len() > 256 {
            return Err(bad_request("q too long"));
        }
    }
    let mut sel = entry::Entity::find()
        .join(JoinType::InnerJoin, entry::Relation::Feed.def())
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
        let backend = st.db.get_database_backend();
        let pq = crate::search::parse_query(qstr);
        if search::is_pg(backend) {
            if let Some(ref g) = pq.general {
                sel = sel.filter(search::fts_filter_expr_pg(g));
                // Miniflux 对齐：有搜索时默认按相关性排序；若显式指定 sort_by 则按指定排序
                let want_rank = q
                    .sorting
                    .sort_by
                    .as_deref()
                    .map(|s| s == "relevance")
                    .unwrap_or(true);
                if want_rank {
                    let ord = match q.sorting.order.as_deref() {
                        Some("asc") => Order::Asc,
                        _ => Order::Desc,
                    };
                    sel = sel
                        .order_by(search::fts_rank_expr_pg(g), ord)
                        .order_by_desc(entry::Column::PublishedAt)
                        .order_by_desc(entry::Column::CreatedAt);
                }
            }
            for v in &pq.title {
                sel = sel.filter(search::fts_field_expr_pg("title", v));
            }
            for v in &pq.author {
                sel = sel.filter(search::fts_field_expr_pg("author", v));
            }
            for v in &pq.url {
                sel = sel.filter(search::fts_field_expr_pg("url", v));
            }
            if !pq.tags.is_empty() {
                let mut tag_cond = Condition::any();
                for t in &pq.tags {
                    tag_cond = tag_cond.add(search::tag_exists_expr_pg(t));
                }
                sel = sel.filter(tag_cond);
            }
        } else {
            // 非 PG 回退：LIKE 匹配
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
                    tag_cond = tag_cond.add(search::tag_exists_expr_like(t));
                }
                sel = sel.filter(tag_cond);
            }
        }
    }
    match q.sorting.sort_by.as_deref() {
        Some("created_at") => {
            sel = match q.sorting.order.as_deref() {
                Some("asc") => sel.order_by_asc(entry::Column::CreatedAt),
                _ => sel.order_by_desc(entry::Column::CreatedAt),
            };
        }
        _ => {
            sel = match q.sorting.order.as_deref() {
                Some("asc") => sel.order_by_asc(entry::Column::PublishedAt),
                _ => sel.order_by_desc(entry::Column::PublishedAt),
            };
            sel = sel.order_by_desc(entry::Column::CreatedAt);
        }
    }
    let l = q.paging.limit.unwrap_or(100);
    sel = sel.limit(l);
    if let Some(o) = q.paging.offset {
        sel = sel.offset(o);
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
    let mut sel = entry::Entity::find()
        .join(JoinType::InnerJoin, entry::Relation::Feed.def())
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
