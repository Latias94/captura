use axum::{
    extract::{Path, State},
    response::Response,
    Json,
};
use axum_extra::typed_header::TypedHeader;
use chrono::{FixedOffset, Utc};
use headers::authorization::Bearer;
use headers::Authorization;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use url::Url;

use crate::auth::AuthUser;
use crate::error::{bad_request, internal, not_found, ApiResult};
use crate::AppState;
use captura_storage::entity::{feed, prelude::*};

#[derive(serde::Serialize)]
pub(crate) struct FaviconResp {
    pub favicon_id: i64,
    pub updated: bool,
}

#[cfg(not(test))]
async fn fetch_favicon(site: &str) -> anyhow::Result<(Vec<u8>, Option<String>, String)> {
    let mut base = Url::parse(site)?;
    base.set_path("/favicon.ico");
    base.set_query(None);
    base.set_fragment(None);
    let cli = reqwest::Client::builder()
        .user_agent("captura/0.1")
        .build()?;
    let res = cli.get(base.as_str()).send().await?;
    if !res.status().is_success() {
        anyhow::bail!("http status {}", res.status());
    }
    let mime = res
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let bytes = res.bytes().await?.to_vec();
    Ok((bytes, mime, base.to_string()))
}

#[cfg(test)]
pub(crate) static TEST_FAVICON_RESP: once_cell::sync::OnceCell<(Vec<u8>, Option<String>)> =
    once_cell::sync::OnceCell::new();

#[cfg(test)]
async fn fetch_favicon(_site: &str) -> anyhow::Result<(Vec<u8>, Option<String>, String)> {
    let (bytes, mime) = TEST_FAVICON_RESP
        .get()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("no test favicon set"))?;
    Ok((bytes, mime, "test://favicon.ico".to_string()))
}

pub(crate) async fn refresh(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(id): Path<i64>,
) -> ApiResult<Json<FaviconResp>> {
    use captura_storage::entity::favicon as fv;
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let Some(f) = Feed::find()
        .filter(feed::Column::Id.eq(id))
        .filter(feed::Column::UserId.eq(user.user_id))
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("feed not found"));
    };
    let site = match f.site_url.clone().or_else(|| Some(f.feed_url.clone())) {
        Some(s) => s,
        None => return Err(bad_request("no site_url/feed_url")),
    };
    let (bytes, mime, actual_url) = fetch_favicon(&site)
        .await
        .map_err(|_| not_found("favicon not found"))?;
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let am = fv::ActiveModel {
        feed_id: Set(Some(f.id)),
        url: Set(Some(actual_url)),
        mime: Set(mime),
        data: Set(Some(bytes)),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    let fav = am.insert(&st.db).await.map_err(internal)?;
    let mut fm: feed::ActiveModel = f.into();
    fm.favicon_id = Set(Some(fav.id));
    fm.update(&st.db).await.map_err(internal)?;
    Ok(Json(FaviconResp {
        favicon_id: fav.id,
        updated: true,
    }))
}

pub(crate) async fn get(
    State(st): State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(id): Path<i64>,
) -> ApiResult<Response> {
    use captura_storage::entity::favicon as fv;
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let Some(f) = fv::Entity::find_by_id(id)
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("favicon not found"));
    };
    if let Some(fid) = f.feed_id {
        let owned = Feed::find_by_id(fid)
            .filter(feed::Column::UserId.eq(user.user_id))
            .one(&st.db)
            .await
            .map_err(internal)?
            .is_some();
        if !owned {
            return Err(not_found("favicon not found"));
        }
    } else {
        return Err(not_found("favicon not found"));
    }
    use axum::body::Bytes;
    let body = Bytes::from(f.data.unwrap_or_default());
    let mut resp = Response::new(body.into());
    if let Some(ct) = f.mime {
        resp.headers_mut().insert(
            axum::http::header::CONTENT_TYPE,
            axum::http::HeaderValue::from_str(&ct)
                .unwrap_or(axum::http::HeaderValue::from_static("image/x-icon")),
        );
    }
    Ok(resp)
}
