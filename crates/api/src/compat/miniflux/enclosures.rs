use super::error::{from_api_error, internal, MfResult};
use crate::auth::mf_auth;
use crate::error::not_found;
use crate::AppState;
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::Json;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QuerySelect, RelationTrait, Set,
};

use captura_storage::entity::{enclosure, entry, feed};

#[derive(serde::Serialize)]
pub(crate) struct MfEnclosureDtoFull {
    pub id: i64,
    #[serde(rename = "user_id")]
    pub user_id: i64,
    #[serde(rename = "entry_id")]
    pub entry_id: i64,
    pub url: String,
    #[serde(rename = "mime_type")]
    pub mime_type: String,
    pub size: i64,
    #[serde(rename = "media_progression")]
    pub media_progression: i64,
}

pub(crate) async fn get(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
) -> MfResult<Json<MfEnclosureDtoFull>> {
    let auth = mf_auth(&st, &headers).await.map_err(from_api_error)?;
    let Some(en) = enclosure::Entity::find_by_id(id)
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("enclosure").into());
    };
    let owned = entry::Entity::find_by_id(en.entry_id)
        .join(sea_orm::JoinType::InnerJoin, entry::Relation::Feed.def())
        .filter(feed::Column::UserId.eq(auth.user_id))
        .one(&st.db)
        .await
        .map_err(internal)?
        .is_some();
    if !owned {
        return Err(not_found("enclosure").into());
    }
    Ok(Json(MfEnclosureDtoFull {
        id: en.id,
        user_id: auth.user_id,
        entry_id: en.entry_id,
        url: en.url,
        mime_type: en.mime.unwrap_or_default(),
        size: en.length.unwrap_or(0),
        media_progression: en.media_progression.unwrap_or(0),
    }))
}

#[derive(serde::Deserialize)]
pub(crate) struct MfEnclosureUpdate {
    #[serde(rename = "media_progression")]
    pub media_progression: i64,
}

pub(crate) async fn update(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<MfEnclosureUpdate>,
) -> MfResult<axum::response::Response> {
    let auth = mf_auth(&st, &headers).await.map_err(from_api_error)?;
    let Some(en) = enclosure::Entity::find_by_id(id)
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("enclosure").into());
    };
    let owned = entry::Entity::find_by_id(en.entry_id)
        .join(sea_orm::JoinType::InnerJoin, entry::Relation::Feed.def())
        .filter(feed::Column::UserId.eq(auth.user_id))
        .one(&st.db)
        .await
        .map_err(internal)?
        .is_some();
    if !owned {
        return Err(not_found("enclosure").into());
    }
    let mut am: enclosure::ActiveModel = en.into();
    am.media_progression = Set(Some(body.media_progression));
    let _ = am.update(&st.db).await.map_err(internal)?;
    Ok((
        axum::http::StatusCode::NO_CONTENT,
        axum::body::Body::empty(),
    )
        .into_response())
}
