use axum::{
    extract::{Path, Query, State},
    Json,
};
use axum_extra::typed_header::TypedHeader;
use chrono::FixedOffset;
use headers::authorization::Bearer;
use headers::Authorization;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, DatabaseConnection, EntityTrait, JoinType, Order,
    QueryFilter, QueryOrder, QuerySelect, RelationTrait, Set,
};
use serde::Deserialize;

use crate::auth::AuthUser;
use crate::entry_options::{apply_entry_flags, EntryUpdateFlags};
use crate::error::{bad_request, internal, ApiResult};
use crate::search;
use crate::util::{validate_limit_offset, validate_sort};
use crate::AppState;

use captura_pipeline::extractor;
use captura_storage::entity::{entry, feed};
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
    if let Some(cond) = captura_service::query::view_filter_condition(q.view) {
        sel = sel.filter(cond);
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
                // Align with Miniflux: when searching, default to relevance; only use sort_by when explicitly requested
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

pub(crate) async fn get_entry(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(id): Path<i64>,
) -> ApiResult<Json<EntryDto>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let Some(e) = load_owned_entry(&st.db, user.user_id, id).await? else {
        return Err(crate::error::not_found("entry"));
    };
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
