use axum::{
    extract::{Path, State},
    Json,
};
use axum_extra::typed_header::TypedHeader;
use chrono::{FixedOffset, Utc};
use headers::authorization::Bearer;
use headers::Authorization;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect, Set,
};
use serde::Deserialize;

use captura_storage::entity::category;

use crate::auth::AuthUser;
use crate::error::{bad_request, internal, not_found, ApiResult};
use crate::AppState;
use captura_types::{CategoryCounterDto, CategoryDto, IdResp};

pub(crate) async fn list_categories(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
) -> ApiResult<Json<Vec<CategoryDto>>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let list = category::Entity::find()
        .filter(category::Column::UserId.eq(user.user_id))
        .order_by_asc(category::Column::Id)
        .all(&st.db)
        .await
        .map_err(internal)?;
    Ok(Json(
        list.into_iter()
            .map(|c| CategoryDto {
                id: c.id,
                name: c.name,
            })
            .collect(),
    ))
}

#[derive(Deserialize)]
pub(crate) struct CreateCategoryReq {
    pub name: String,
}

pub(crate) async fn create_category(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Json(body): Json<CreateCategoryReq>,
) -> ApiResult<Json<IdResp>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let name = body.name.trim();
    if name.is_empty() || name.len() > 128 {
        return Err(bad_request("invalid category name"));
    }
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let am = category::ActiveModel {
        user_id: Set(user.user_id),
        name: Set(name.to_string()),
        created_at: Set(now),
        ..Default::default()
    };
    let c = am.insert(&st.db).await.map_err(internal)?;
    Ok(Json(IdResp { id: c.id }))
}

#[derive(Deserialize)]
pub(crate) struct UpdateCategoryReq {
    pub name: String,
}

pub(crate) async fn get_category(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(id): Path<i64>,
) -> ApiResult<Json<CategoryDto>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let Some(c) = category::Entity::find()
        .filter(category::Column::UserId.eq(user.user_id))
        .filter(category::Column::Id.eq(id))
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("category not found"));
    };
    Ok(Json(CategoryDto {
        id: c.id,
        name: c.name,
    }))
}

pub(crate) async fn update_category(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(id): Path<i64>,
    Json(body): Json<UpdateCategoryReq>,
) -> ApiResult<&'static str> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let Some(c) = category::Entity::find()
        .filter(category::Column::UserId.eq(user.user_id))
        .filter(category::Column::Id.eq(id))
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("category not found"));
    };
    let mut am: category::ActiveModel = c.into();
    let name = body.name.trim();
    if name.is_empty() || name.len() > 128 {
        return Err(bad_request("invalid category name"));
    }
    am.name = Set(name.to_string());
    am.update(&st.db).await.map_err(internal)?;
    Ok("ok")
}

pub(crate) async fn delete_category(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(id): Path<i64>,
) -> ApiResult<&'static str> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let Some(c) = category::Entity::find()
        .filter(category::Column::UserId.eq(user.user_id))
        .filter(category::Column::Id.eq(id))
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("category not found"));
    };
    let am: category::ActiveModel = c.into();
    am.delete(&st.db).await.map_err(internal)?;
    Ok("ok")
}

/// 统计当前用户下各分类的未读数（与 Miniflux 语义类似，category_id 为 None 表示未分类）。
pub(crate) async fn category_counters(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
) -> ApiResult<Json<Vec<CategoryCounterDto>>> {
    use captura_storage::entity::{entry, feed};
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let feeds = feed::Entity::find()
        .filter(feed::Column::UserId.eq(user.user_id))
        .all(&st.db)
        .await
        .map_err(internal)?;
    let feed_ids: Vec<i64> = feeds.iter().map(|f| f.id).collect();
    if feed_ids.is_empty() {
        return Ok(Json(vec![]));
    }
    let pairs: Vec<(i64, i64)> = entry::Entity::find()
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
    use std::collections::HashMap;
    let feed_cat: HashMap<i64, Option<i64>> =
        feeds.into_iter().map(|f| (f.id, f.category_id)).collect();
    let mut cat_map: HashMap<Option<i64>, i64> = HashMap::new();
    for (fid, cnt) in pairs {
        let cat = feed_cat.get(&fid).cloned().unwrap_or(None);
        *cat_map.entry(cat).or_insert(0) += cnt;
    }
    let out = cat_map
        .into_iter()
        .map(|(category_id, unread)| CategoryCounterDto { category_id, unread })
        .collect();
    Ok(Json(out))
}
