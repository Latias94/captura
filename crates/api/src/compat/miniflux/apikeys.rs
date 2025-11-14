use super::error::{from_api_error, internal, MfResult};
use crate::auth::mf_auth;
use crate::AppState;
use axum::extract::{Path, State};
use axum::Json;
use base64::Engine as _;
use chrono::{FixedOffset, Utc};
use rand_core::{OsRng, RngCore};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use sha2::Digest;

use captura_storage::entity::token;

#[derive(serde::Serialize)]
pub(crate) struct MfApiKeyDto {
    pub id: i64,
    #[serde(rename = "user_id")]
    pub user_id: i64,
    pub token: String,
    pub description: Option<String>,
    #[serde(rename = "last_used_at")]
    pub last_used_at: Option<String>,
    #[serde(rename = "created_at")]
    pub created_at: String,
}

#[derive(serde::Deserialize)]
pub(crate) struct MfCreateApiKeyReq {
    pub description: Option<String>,
}

pub(crate) async fn list(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
) -> MfResult<Json<Vec<MfApiKeyDto>>> {
    let auth = mf_auth(&st, &headers).await.map_err(from_api_error)?;
    let keys = token::Entity::find()
        .filter(token::Column::UserId.eq(auth.user_id))
        .all(&st.db)
        .await
        .map_err(internal)?;
    let mut list = Vec::new();
    for k in keys {
        list.push(MfApiKeyDto {
            id: k.id,
            user_id: k.user_id,
            token: k.token_plain.unwrap_or_default(),
            description: k.name,
            last_used_at: k.last_used_at.map(|d| d.to_rfc3339()),
            created_at: k.created_at.to_rfc3339(),
        });
    }
    Ok(Json(list))
}

pub(crate) async fn create(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(body): Json<MfCreateApiKeyReq>,
) -> MfResult<Json<MfApiKeyDto>> {
    let auth = mf_auth(&st, &headers).await.map_err(from_api_error)?;
    let mut rand_bytes = [0u8; 32];
    OsRng.fill_bytes(&mut rand_bytes);
    let token_str = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(rand_bytes);
    let token_hash = format!("{:x}", sha2::Sha256::digest(token_str.as_bytes()));
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let am = token::ActiveModel {
        user_id: Set(auth.user_id),
        name: Set(body.description.clone()),
        token_hash: Set(token_hash),
        token_plain: Set(Some(token_str.clone())),
        created_at: Set(now),
        last_used_at: Set(None),
        expires_at: Set(None),
        ..Default::default()
    };
    let k = am.insert(&st.db).await.map_err(internal)?;
    Ok(Json(MfApiKeyDto {
        id: k.id,
        user_id: auth.user_id,
        token: token_str,
        description: k.name,
        last_used_at: None,
        created_at: k.created_at.to_rfc3339(),
    }))
}

pub(crate) async fn delete(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
) -> MfResult<&'static str> {
    let auth = mf_auth(&st, &headers).await.map_err(from_api_error)?;
    if let Some(k) = token::Entity::find_by_id(id)
        .filter(token::Column::UserId.eq(auth.user_id))
        .one(&st.db)
        .await
        .map_err(internal)?
    {
        let am: token::ActiveModel = k.into();
        let _ = am.delete(&st.db).await.map_err(internal)?;
    }
    Ok("ok")
}
