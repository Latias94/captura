use axum::{
    extract::{Path, State},
    Json,
};
use axum_extra::typed_header::TypedHeader;
use chrono::{FixedOffset, Utc};
use headers::authorization::Bearer;
use headers::Authorization;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::error::{bad_request, internal, not_found, ApiResult};
use crate::{AppState, IdResp};
use captura_storage::entity::integration;

#[derive(Serialize)]
pub(crate) struct IntegrationDto {
    pub id: i64,
    pub kind: String,
    pub enabled: bool,
    pub config_json: serde_json::Value,
    pub created_at: String,
}

#[derive(Deserialize)]
pub(crate) struct CreateIntegrationReq {
    pub kind: String,
    pub enabled: Option<bool>,
    pub config_json: serde_json::Value,
}

pub(crate) async fn list(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
) -> ApiResult<Json<Vec<IntegrationDto>>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let list = integration::Entity::find()
        .filter(integration::Column::UserId.eq(user.user_id))
        .all(&st.db)
        .await
        .map_err(internal)?;
    Ok(Json(
        list.into_iter()
            .map(|m| IntegrationDto {
                id: m.id,
                kind: m.kind,
                enabled: m.enabled,
                config_json: m.config_json.unwrap_or(serde_json::json!({})),
                created_at: m.created_at.to_rfc3339(),
            })
            .collect(),
    ))
}

pub(crate) async fn get(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(id): Path<i64>,
) -> ApiResult<Json<IntegrationDto>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let Some(m) = integration::Entity::find_by_id(id)
        .filter(integration::Column::UserId.eq(user.user_id))
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("integration"));
    };
    Ok(Json(IntegrationDto {
        id: m.id,
        kind: m.kind,
        enabled: m.enabled,
        config_json: m.config_json.unwrap_or(serde_json::json!({})),
        created_at: m.created_at.to_rfc3339(),
    }))
}

pub(crate) async fn create(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Json(req): Json<CreateIntegrationReq>,
) -> ApiResult<Json<IdResp>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    if req.kind.trim().is_empty() {
        return Err(bad_request("kind required"));
    }
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let am = integration::ActiveModel {
        user_id: Set(user.user_id),
        kind: Set(req.kind.clone()),
        config_json: Set(Some(req.config_json.clone())),
        enabled: Set(req.enabled.unwrap_or(true)),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    let m = am.insert(&st.db).await.map_err(internal)?;
    Ok(Json(IdResp { id: m.id }))
}

#[derive(Deserialize)]
pub(crate) struct UpdateIntegrationReq {
    pub enabled: Option<bool>,
    pub config_json: Option<serde_json::Value>,
}

pub(crate) async fn update(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(id): Path<i64>,
    Json(req): Json<UpdateIntegrationReq>,
) -> ApiResult<&'static str> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let Some(m) = integration::Entity::find_by_id(id)
        .filter(integration::Column::UserId.eq(user.user_id))
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("integration"));
    };
    let mut am: integration::ActiveModel = m.into();
    if let Some(en) = req.enabled {
        am.enabled = Set(en);
    }
    if let Some(cfg) = req.config_json {
        am.config_json = Set(Some(cfg));
    }
    am.updated_at = Set(Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap()));
    let _ = am.update(&st.db).await.map_err(internal)?;
    Ok("ok")
}

pub(crate) async fn delete(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(id): Path<i64>,
) -> ApiResult<&'static str> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    if let Some(m) = integration::Entity::find_by_id(id)
        .filter(integration::Column::UserId.eq(user.user_id))
        .one(&st.db)
        .await
        .map_err(internal)?
    {
        let am: integration::ActiveModel = m.into();
        let _ = am.delete(&st.db).await.map_err(internal)?;
    }
    Ok("ok")
}
