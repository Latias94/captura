use super::error::{from_api_error, internal, MfResult};
use crate::auth::mf_auth;
use crate::AppState;
use axum::extract::State;
use axum::Json;
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};

#[derive(serde::Serialize)]
pub(crate) struct MfIntegrationsStatus {
    #[serde(rename = "has_integrations")]
    pub has_integrations: bool,
}

pub(crate) async fn status(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
) -> MfResult<Json<MfIntegrationsStatus>> {
    let auth = mf_auth(&st, &headers).await.map_err(from_api_error)?;
    use captura_storage::entity::integration;
    let count = integration::Entity::find()
        .filter(integration::Column::UserId.eq(auth.user_id))
        .filter(integration::Column::Enabled.eq(true))
        .count(&st.db)
        .await
        .map_err(internal)?;
    Ok(Json(MfIntegrationsStatus {
        has_integrations: count > 0,
    }))
}
