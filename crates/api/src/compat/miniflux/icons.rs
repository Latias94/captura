use super::error::MfResult;
use crate::auth::mf_auth;
use crate::error::{internal, not_found};
use crate::AppState;
use axum::extract::{Path, State};
use axum::response::IntoResponse;

use captura_storage::entity::feed;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

pub(crate) async fn icon_by_id(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
) -> MfResult<axum::response::Response> {
    let _auth = mf_auth(&st, &headers).await?;
    use captura_storage::entity::favicon as fv;
    let Some(v) = fv::Entity::find_by_id(id)
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("icon").into());
    };
    let bytes = v.data.unwrap_or_default();
    let mime = v.mime.unwrap_or_else(|| "image/x-icon".into());
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_str(&mime).unwrap_or(axum::http::HeaderValue::from_static(
            "application/octet-stream",
        )),
    );
    Ok((headers, bytes).into_response())
}

pub(crate) async fn icon_by_feed(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
) -> MfResult<axum::response::Response> {
    let auth = mf_auth(&st, &headers).await?;
    use captura_storage::entity::favicon as fv;
    let Some(f) = feed::Entity::find_by_id(id)
        .filter(feed::Column::UserId.eq(auth.user_id))
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("feed").into());
    };
    let Some(fid) = f.favicon_id else {
        return Err(not_found("icon").into());
    };
    let Some(v) = fv::Entity::find_by_id(fid)
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("icon").into());
    };
    let bytes = v.data.unwrap_or_default();
    let mime = v.mime.unwrap_or_else(|| "image/x-icon".into());
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_str(&mime).unwrap_or(axum::http::HeaderValue::from_static(
            "application/octet-stream",
        )),
    );
    Ok((headers, bytes).into_response())
}
