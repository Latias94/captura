use super::error::MfResult;
use crate::auth::mf_auth;
use crate::error::internal;
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
    let auth = mf_auth(&st, &headers).await?;
    use captura_storage::entity::{integration, prelude::*};
    let count = Integration::find()
        .filter(integration::Column::UserId.eq(auth.user_id))
        .filter(integration::Column::Enabled.eq(true))
        .count(&st.db)
        .await
        .map_err(internal)?;
    Ok(Json(MfIntegrationsStatus {
        has_integrations: count > 0,
    }))
}
