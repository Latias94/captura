use axum::{
    extract::{Path, State},
    Json,
};
use axum_extra::typed_header::TypedHeader;
use chrono::{FixedOffset, Utc};
use headers::authorization::Bearer;
use headers::Authorization;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use serde::{Deserialize, Serialize};

use captura_storage::entity::{category, prelude::*};

use crate::auth::AuthUser;
use crate::error::{bad_request, internal, not_found, ApiResult};
use crate::AppState;

#[derive(Serialize)]
pub(crate) struct CategoryDto {
    pub id: i64,
    pub name: String,
}

pub(crate) async fn list_categories(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
) -> ApiResult<Json<Vec<CategoryDto>>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let list = Category::find()
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
) -> ApiResult<Json<crate::IdResp>> {
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
    Ok(Json(crate::IdResp { id: c.id }))
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
    let Some(c) = Category::find()
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
    let Some(c) = Category::find()
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
    let Some(c) = Category::find()
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
