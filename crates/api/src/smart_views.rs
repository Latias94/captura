use axum::{
    Json,
    extract::{Path, Query, State},
};
use axum_extra::typed_header::TypedHeader;
use chrono::{FixedOffset, Utc};
use headers::Authorization;
use headers::authorization::Bearer;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use serde::Deserialize;

use crate::AppState;
use crate::auth::AuthUser;
use crate::error::{ApiResult, bad_request, internal, not_found};
use crate::util::{map_entry_to_dto, validate_limit_offset, validate_sort};
use captura_service::query::{TimelineQuery, TimelineStatus, list_entries_for_user};
use captura_storage::entity::smart_view;
use captura_types::{EntryDto, EntryView, SmartViewDto, SmartViewFiltersDto, Sorting};

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
    validate_sort(
        &body.sort_by,
        &["published_at", "created_at"],
        &body.sort_order,
    )?;
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
    pub limit: Option<u64>,
    pub offset: Option<u64>,
}

pub(crate) async fn list_smart_view_entries(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(id): Path<i64>,
    Query(q): Query<SmartViewEntriesQuery>,
) -> ApiResult<Json<Vec<EntryDto>>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    validate_limit_offset(q.limit, q.offset)?;
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

    let mut feed_ids = filters.feed_ids.unwrap_or_default();
    let mut category_ids = filters.category_ids.unwrap_or_default();
    let mut label_ids = filters.label_ids.unwrap_or_default();
    // Normalize potential None values to empty vectors.
    if feed_ids.is_empty() {
        feed_ids = Vec::new();
    }
    if category_ids.is_empty() {
        category_ids = Vec::new();
    }
    if label_ids.is_empty() {
        label_ids = Vec::new();
    }
    let status = filters.status.as_deref().and_then(|s| match s {
        "read" => Some(TimelineStatus::Read),
        "unread" => Some(TimelineStatus::Unread),
        "starred" => Some(TimelineStatus::Starred),
        _ => None,
    });
    let limit = q.limit.unwrap_or(100);
    let offset = q.offset.unwrap_or(0);
    let tquery = TimelineQuery::new(
        Some(view),
        feed_ids,
        category_ids,
        label_ids,
        status,
        filters.search.clone(),
        Some(sort_by.to_string()),
        Some(sort_order.to_string()),
        limit,
        offset,
        None,
        None,
    );
    let list = list_entries_for_user(&st.db, user.user_id, &tquery)
        .await
        .map_err(internal)?;
    Ok(Json(
        list.into_iter()
            .map(|e| map_entry_to_dto(e, None))
            .collect(),
    ))
}
