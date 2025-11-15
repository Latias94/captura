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
use crate::AppState;
use captura_storage::entity::webhook;
use captura_types::IdResp;
use rand_core::RngCore;

#[derive(Serialize)]
pub(crate) struct WebhookDto {
    pub id: i64,
    pub url: String,
    pub events: Option<String>,
    pub enabled: bool,
    pub created_at: String,
}

#[derive(Deserialize)]
pub(crate) struct CreateWebhookReq {
    pub url: String,
    pub events: Option<String>,
}

pub(crate) async fn list(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
) -> ApiResult<Json<Vec<WebhookDto>>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let list = webhook::Entity::find()
        .filter(webhook::Column::UserId.eq(user.user_id))
        .all(&st.db)
        .await
        .map_err(internal)?;
    Ok(Json(
        list.into_iter()
            .map(|w| WebhookDto {
                id: w.id,
                url: w.url,
                events: w.events,
                enabled: w.enabled,
                created_at: w.created_at.to_rfc3339(),
            })
            .collect(),
    ))
}

pub(crate) async fn get(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(id): Path<i64>,
) -> ApiResult<Json<WebhookDto>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let Some(w) = webhook::Entity::find_by_id(id)
        .filter(webhook::Column::UserId.eq(user.user_id))
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("webhook not found"));
    };
    Ok(Json(WebhookDto {
        id: w.id,
        url: w.url,
        events: w.events,
        enabled: w.enabled,
        created_at: w.created_at.to_rfc3339(),
    }))
}

pub(crate) async fn create(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Json(body): Json<CreateWebhookReq>,
) -> ApiResult<Json<IdResp>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    if body.url.trim().is_empty() {
        return Err(bad_request("url required"));
    }
    // Generate webhook secret
    let mut rand_bytes = [0u8; 32];
    rand_core::OsRng.fill_bytes(&mut rand_bytes);
    let secret = hex::encode(rand_bytes);
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let am = webhook::ActiveModel {
        user_id: Set(user.user_id),
        url: Set(body.url.clone()),
        secret: Set(secret),
        events: Set(body.events.clone()),
        enabled: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    let w = am.insert(&st.db).await.map_err(internal)?;
    Ok(Json(IdResp { id: w.id }))
}

pub(crate) async fn delete(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(id): Path<i64>,
) -> ApiResult<&'static str> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    if let Some(w) = webhook::Entity::find_by_id(id)
        .filter(webhook::Column::UserId.eq(user.user_id))
        .one(&st.db)
        .await
        .map_err(internal)?
    {
        let am: webhook::ActiveModel = w.into();
        let _ = am.delete(&st.db).await.map_err(internal)?;
    }
    Ok("ok")
}
