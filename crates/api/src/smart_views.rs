use axum::{
    extract::{Path, Query, State},
    Json,
};
use axum_extra::typed_header::TypedHeader;
use chrono::{FixedOffset, Utc};
use headers::authorization::Bearer;
use headers::Authorization;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, EntityTrait, JoinType, QueryFilter, QueryOrder,
    QuerySelect, RelationTrait, Set,
};
use serde::Deserialize;

use crate::auth::AuthUser;
use crate::error::{bad_request, internal, not_found, ApiResult};
use crate::search;
use crate::util::{validate_limit_offset, validate_sort};
use crate::AppState;
use captura_storage::entity::{entry, entry_label, feed, smart_view};
use captura_types::{
    EntryDto, EntryView, Paging, SmartViewDto, SmartViewFiltersDto, Sorting,
};

#[derive(Deserialize)]
pub(crate) struct SmartViewCreateReq {
    pub name: String,
    pub view: EntryView,
    #[serde(default)]
    pub filters: SmartViewFiltersDto,
    pub sort_by: Option<String>,
    pub sort_order: Option<String>,
    pub pinned: Option<bool>,
}

#[derive(Deserialize, Default)]
pub(crate) struct SmartViewUpdateReq {
    pub name: Option<String>,
    pub view: Option<EntryView>,
    pub filters: Option<SmartViewFiltersDto>,
    pub sort_by: Option<String>,
    pub sort_order: Option<String>,
    pub pinned: Option<bool>,
}

fn map_model_to_dto(m: smart_view::Model) -> SmartViewDto {
    let view = smart_view_view_to_enum(&m.view);
    let filters: SmartViewFiltersDto = m
        .filters_json
        .as_ref()
        .and_then(|j| serde_json::from_value(j.clone()).ok())
        .unwrap_or_default();
    SmartViewDto {
        id: m.id,
        name: m.name,
        view,
        filters,
        sort_by: m.sort_by,
        sort_order: m.sort_order,
        pinned: m.pinned,
    }
}

fn smart_view_view_to_enum(s: &str) -> EntryView {
    EntryView::from_str(s).unwrap_or(EntryView::Articles)
}

fn smart_view_view_to_string(v: EntryView) -> String {
    v.as_str().to_string()
}

pub(crate) async fn list_smart_views(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
) -> ApiResult<Json<Vec<SmartViewDto>>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let list = smart_view::Entity::find()
        .filter(smart_view::Column::UserId.eq(user.user_id))
        .order_by_asc(smart_view::Column::Id)
        .all(&st.db)
        .await
        .map_err(internal)?;
    Ok(Json(list.into_iter().map(map_model_to_dto).collect()))
}

pub(crate) async fn create_smart_view(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Json(body): Json<SmartViewCreateReq>,
) -> ApiResult<Json<SmartViewDto>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let name = body.name.trim();
    if name.is_empty() || name.len() > 190 {
        return Err(bad_request("invalid smart view name"));
    }
    validate_sort(&body.sort_by, &["published_at", "created_at"], &body.sort_order)?;
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let filters_json = serde_json::to_value(&body.filters).map_err(internal)?;
    let am = smart_view::ActiveModel {
        user_id: Set(user.user_id),
        name: Set(name.to_string()),
        view: Set(smart_view_view_to_string(body.view)),
        filters_json: Set(Some(filters_json)),
        sort_by: Set(body.sort_by.clone()),
        sort_order: Set(body.sort_order.clone()),
        pinned: Set(body.pinned.unwrap_or(false)),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    let sv = am.insert(&st.db).await.map_err(internal)?;
    Ok(Json(map_model_to_dto(sv)))
}

pub(crate) async fn get_smart_view(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(id): Path<i64>,
) -> ApiResult<Json<SmartViewDto>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let Some(sv) = smart_view::Entity::find()
        .filter(smart_view::Column::UserId.eq(user.user_id))
        .filter(smart_view::Column::Id.eq(id))
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("smart view not found"));
    };
    Ok(Json(map_model_to_dto(sv)))
}

pub(crate) async fn update_smart_view(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(id): Path<i64>,
    Json(body): Json<SmartViewUpdateReq>,
) -> ApiResult<Json<SmartViewDto>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let Some(sv) = smart_view::Entity::find()
        .filter(smart_view::Column::UserId.eq(user.user_id))
        .filter(smart_view::Column::Id.eq(id))
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("smart view not found"));
    };
    let mut am: smart_view::ActiveModel = sv.into();
    if let Some(name_raw) = body.name {
        let name = name_raw.trim();
        if name.is_empty() || name.len() > 190 {
            return Err(bad_request("invalid smart view name"));
        }
        am.name = Set(name.to_string());
    }
    if let Some(v) = body.view {
        am.view = Set(smart_view_view_to_string(v));
    }
    if let Some(filters) = body.filters {
        let json = serde_json::to_value(filters).map_err(internal)?;
        am.filters_json = Set(Some(json));
    }
    if body.sort_by.is_some() || body.sort_order.is_some() {
        validate_sort(
            &body.sort_by,
            &["published_at", "created_at"],
            &body.sort_order,
        )?;
        if let Some(sb) = body.sort_by {
            am.sort_by = Set(Some(sb));
        }
        if let Some(so) = body.sort_order {
            am.sort_order = Set(Some(so));
        }
    }
    if let Some(p) = body.pinned {
        am.pinned = Set(p);
    }
    am.updated_at = Set(Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap()));
    let sv = am.update(&st.db).await.map_err(internal)?;
    Ok(Json(map_model_to_dto(sv)))
}

pub(crate) async fn delete_smart_view(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(id): Path<i64>,
) -> ApiResult<&'static str> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let Some(sv) = smart_view::Entity::find()
        .filter(smart_view::Column::UserId.eq(user.user_id))
        .filter(smart_view::Column::Id.eq(id))
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("smart view not found"));
    };
    let am: smart_view::ActiveModel = sv.into();
    am.delete(&st.db).await.map_err(internal)?;
    Ok("ok")
}

#[derive(Deserialize)]
pub(crate) struct SmartViewEntriesQuery {
    #[serde(flatten)]
    pub sorting: Sorting,
    #[serde(flatten)]
    pub paging: Paging,
}

pub(crate) async fn list_smart_view_entries(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(id): Path<i64>,
    Query(q): Query<SmartViewEntriesQuery>,
) -> ApiResult<Json<Vec<EntryDto>>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    validate_limit_offset(q.paging.limit, q.paging.offset)?;
    // Only allow a subset of sort keys for smart views for now.
    validate_sort(
        &q.sorting.sort_by,
        &["published_at", "created_at"],
        &q.sorting.order,
    )?;

    let Some(sv) = smart_view::Entity::find()
        .filter(smart_view::Column::UserId.eq(user.user_id))
        .filter(smart_view::Column::Id.eq(id))
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("smart view not found"));
    };

    let view = smart_view_view_to_enum(&sv.view);
    let filters: SmartViewFiltersDto = sv
        .filters_json
        .as_ref()
        .and_then(|j| serde_json::from_value(j.clone()).ok())
        .unwrap_or_default();

    let mut sel = entry::Entity::find()
        .join(JoinType::InnerJoin, entry::Relation::Feed.def())
        .filter(feed::Column::UserId.eq(user.user_id));

    if let Some(feed_ids) = &filters.feed_ids {
        if !feed_ids.is_empty() {
            sel = sel.filter(entry::Column::FeedId.is_in(feed_ids.clone()));
        }
    }
    if let Some(category_ids) = &filters.category_ids {
        if !category_ids.is_empty() {
            sel = sel.filter(feed::Column::CategoryId.is_in(category_ids.clone()));
        }
    }
    if let Some(label_ids) = &filters.label_ids {
        if !label_ids.is_empty() {
            sel = sel
                .join(JoinType::InnerJoin, entry_label::Relation::Entry.def())
                .filter(entry_label::Column::LabelId.is_in(label_ids.clone()));
        }
    }
    if !matches!(view, EntryView::All) {
        // Same semantics as /api/v1/entries: NULL view is treated as "articles".
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
    if let Some(ref sts) = filters.status {
        match sts.as_str() {
            "read" => sel = sel.filter(entry::Column::IsRead.eq(true)),
            "unread" => sel = sel.filter(entry::Column::IsRead.eq(false)),
            "starred" => sel = sel.filter(entry::Column::IsStarred.eq(true)),
            _ => {}
        }
    }
    if let Some(ref qstr) = filters.search {
        let backend = st.db.get_database_backend();
        let pq = crate::search::parse_query(qstr);
        if search::is_pg(backend) {
            if let Some(ref g) = pq.general {
                sel = sel.filter(search::fts_filter_expr_pg(g));
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

    // Determine effective sort key: query parameter overrides stored preference.
    let sort_by = q
        .sorting
        .sort_by
        .as_deref()
        .or(sv.sort_by.as_deref())
        .unwrap_or("published_at");
    let sort_order = q
        .sorting
        .order
        .as_deref()
        .or(sv.sort_order.as_deref())
        .unwrap_or("desc");

    match sort_by {
        "created_at" => {
            sel = match sort_order {
                "asc" => sel.order_by_asc(entry::Column::CreatedAt),
                _ => sel.order_by_desc(entry::Column::CreatedAt),
            };
        }
        _ => {
            sel = match sort_order {
                "asc" => sel.order_by_asc(entry::Column::PublishedAt),
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
