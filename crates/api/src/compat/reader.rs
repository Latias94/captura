use axum::{
    Form, Json, Router,
    extract::{Query, State},
    routing::{get, post},
};
use axum_extra::typed_header::TypedHeader;
use headers::Authorization;
use headers::authorization::Bearer;
pub(crate) mod handlers;
pub(crate) mod types;
use self::types::*;

use crate::AppState;
use crate::auth::AuthUser;
use crate::error::ApiResult;

// ---------- Endpoints ----------
pub(crate) async fn subscription_list(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    _q: Query<ReaderQuery>,
) -> ApiResult<Json<ReaderSubscriptionListResp>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let resp = handlers::subscription_list(&st, user.user_id).await?;
    Ok(Json(resp))
}

pub(crate) async fn stream_contents(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    q: Query<ReaderQuery>,
) -> ApiResult<Json<ReaderStreamResp>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let resp = handlers::stream_contents(&st, user.user_id, &q).await?;
    Ok(Json(resp))
}

pub(crate) async fn items_ids(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    q: Query<ReaderItemsIdsQuery>,
) -> ApiResult<Json<ReaderItemsIdsResp>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let resp = handlers::items_ids(&st, user.user_id, &q).await?;
    Ok(Json(resp))
}

pub(crate) async fn items_contents(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    q: Query<ReaderItemsContentsQuery>,
) -> ApiResult<Json<ReaderItemsContentsResp>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let resp = handlers::items_contents(&st, user.user_id, &q).await?;
    Ok(Json(resp))
}

pub(crate) async fn edit_tag(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Form(f): Form<ReaderEditTagForm>,
) -> ApiResult<&'static str> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let _ = handlers::edit_tag(&st, user.user_id, &f).await?;
    Ok("OK")
}

pub(crate) async fn mark_all_read(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Form(f): Form<ReaderMarkAllForm>,
) -> ApiResult<&'static str> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let _ = handlers::mark_all_read(&st, user.user_id, &f).await?;
    Ok("OK")
}

pub(crate) async fn unread_count(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
) -> ApiResult<Json<ReaderUnreadCountResp>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let resp = handlers::unread_count(&st, user.user_id).await?;
    Ok(Json(resp))
}

pub(crate) async fn subscription_quickadd(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Form(f): Form<ReaderQuickAddForm>,
) -> ApiResult<Json<ReaderQuickAddResp>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let resp = handlers::subscription_quickadd(&st, user.user_id, &f).await?;
    Ok(Json(resp))
}

pub(crate) async fn subscription_edit(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Form(f): Form<ReaderSubEditForm>,
) -> ApiResult<&'static str> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let _ = handlers::subscription_edit(&st, user.user_id, &f).await?;
    Ok("OK")
}

/// Build the Google Reader compatibility-layer Router (`/reader/api/0/*`).
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/reader/api/0/subscription/list", get(subscription_list))
        .route(
            "/reader/api/0/stream/contents/user/-/state/com.google/reading-list",
            get(stream_contents),
        )
        .route("/reader/api/0/edit-tag", post(edit_tag))
        .route("/reader/api/0/mark-all-as-read", post(mark_all_read))
        .route("/reader/api/0/unread-count", get(unread_count))
        .route(
            "/reader/api/0/subscription/quickadd",
            post(subscription_quickadd),
        )
        .route("/reader/api/0/subscription/edit", post(subscription_edit))
        .route("/reader/api/0/stream/items/ids", get(items_ids))
        .route("/reader/api/0/stream/items/contents", get(items_contents))
}
