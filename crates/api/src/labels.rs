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

use crate::auth::AuthUser;
use crate::error::{bad_request, internal, not_found, ApiResult};
use crate::AppState;
use captura_storage::entity::label;

/// Minimal label DTO exposed by `/api/v1/labels`.
#[derive(Debug, Clone, Serialize)]
pub struct LabelDto {
    pub id: i64,
    pub name: String,
    pub color: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreateLabelReq {
    pub name: String,
    pub color: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct UpdateLabelReq {
    pub name: Option<String>,
    pub color: Option<String>,
}

fn map_label(m: label::Model) -> LabelDto {
    LabelDto {
        id: m.id,
        name: m.name,
        color: m.color,
    }
}

pub(crate) async fn list_labels(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
) -> ApiResult<Json<Vec<LabelDto>>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let list = label::Entity::find()
        .filter(label::Column::UserId.eq(user.user_id))
        .order_by_asc(label::Column::Name)
        .all(&st.db)
        .await
        .map_err(internal)?;
    Ok(Json(list.into_iter().map(map_label).collect()))
}

pub(crate) async fn create_label(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Json(body): Json<CreateLabelReq>,
) -> ApiResult<Json<LabelDto>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let name = body.name.trim();
    if name.is_empty() || name.len() > 190 {
        return Err(bad_request("invalid label name"));
    }
    // Enforce per-user uniqueness on (user_id, name) at the API layer.
    if label::Entity::find()
        .filter(label::Column::UserId.eq(user.user_id))
        .filter(label::Column::Name.eq(name.to_string()))
        .one(&st.db)
        .await
        .map_err(internal)?
        .is_some()
    {
        return Err(bad_request("label with this name already exists"));
    }

    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let am = label::ActiveModel {
        id: Default::default(),
        user_id: Set(user.user_id),
        name: Set(name.to_string()),
        color: Set(body.color.clone()),
        created_at: Set(now),
    };
    let m = am.insert(&st.db).await.map_err(internal)?;
    Ok(Json(map_label(m)))
}

pub(crate) async fn update_label(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(id): Path<i64>,
    Json(body): Json<UpdateLabelReq>,
) -> ApiResult<Json<LabelDto>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let Some(m) = label::Entity::find()
        .filter(label::Column::UserId.eq(user.user_id))
        .filter(label::Column::Id.eq(id))
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("label not found"));
    };
    let mut am: label::ActiveModel = m.into();
    if let Some(name_raw) = body.name {
        let name = name_raw.trim();
        if name.is_empty() || name.len() > 190 {
            return Err(bad_request("invalid label name"));
        }
        // Enforce uniqueness when renaming.
        if label::Entity::find()
            .filter(label::Column::UserId.eq(user.user_id))
            .filter(label::Column::Name.eq(name.to_string()))
            .filter(label::Column::Id.ne(id))
            .one(&st.db)
            .await
            .map_err(internal)?
            .is_some()
        {
            return Err(bad_request("label with this name already exists"));
        }
        am.name = Set(name.to_string());
    }
    if let Some(color) = body.color {
        am.color = Set(Some(color));
    }
    let m = am.update(&st.db).await.map_err(internal)?;
    Ok(Json(map_label(m)))
}

pub(crate) async fn delete_label(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(id): Path<i64>,
) -> ApiResult<&'static str> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let Some(m) = label::Entity::find()
        .filter(label::Column::UserId.eq(user.user_id))
        .filter(label::Column::Id.eq(id))
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("label not found"));
    };
    let am: label::ActiveModel = m.into();
    am.delete(&st.db).await.map_err(internal)?;
    Ok("ok")
}
