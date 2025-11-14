use argon2::PasswordVerifier;
use axum::{extract::State, Json};
use base64::Engine as _;
use chrono::{FixedOffset, Utc};
use rand_core::RngCore;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::auth::{create_user_with_random_password, find_user_id_by_username};
use crate::error::{bad_request, forbidden, internal, unauthorized, ApiResult};
use crate::state::AppState;
use captura_storage::entity::{token, user};

#[derive(Deserialize)]
pub struct AuthLoginReq {
    pub username: String,
    pub password: String,
    pub name: Option<String>,
}

#[derive(Serialize)]
pub struct AuthLoginResp {
    pub token: String,
}

pub async fn auth_login(
    State(st): State<AppState>,
    Json(body): Json<AuthLoginReq>,
) -> ApiResult<Json<AuthLoginResp>> {
    if st.cfg.disable_local_auth {
        return Err(forbidden("local auth disabled"));
    }
    if body.username.trim().is_empty() || body.password.is_empty() {
        return Err(bad_request("username/password required"));
    }
    let key = body.username.to_lowercase();
    if crate::util::login_check_and_mark(
        &key,
        st.cfg.login_max_attempts,
        st.cfg.login_window_secs,
        false,
    )
    .is_err()
    {
        return Err(crate::error::too_many_requests("too many attempts"));
    }
    let u = user::Entity::find()
        .filter(user::Column::Username.eq(&body.username))
        .one(&st.db)
        .await
        .map_err(internal)?;
    let Some(u) = u else {
        return Err(unauthorized("invalid credentials"));
    };
    let parsed = argon2::PasswordHash::new(&u.password_hash).map_err(internal)?;
    argon2::Argon2::default()
        .verify_password(body.password.as_bytes(), &parsed)
        .map_err(|_| unauthorized("invalid credentials"))?;
    let mut rand_bytes = [0u8; 32];
    rand_core::OsRng.fill_bytes(&mut rand_bytes);
    let token_str = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(rand_bytes);
    let token_hash = format!("{:x}", Sha256::digest(token_str.as_bytes()));
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let am = token::ActiveModel {
        user_id: Set(u.id),
        name: Set(body.name),
        token_hash: Set(token_hash),
        token_plain: Set(Some(token_str.clone())),
        created_at: Set(now),
        last_used_at: Set(Some(now)),
        expires_at: Set(Some(
            now + chrono::Duration::seconds({
                let v: i64 = std::env::var("CAPTURA_TOKEN_TTL_SECS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(30 * 24 * 3600);
                v.max(3600)
            }),
        )),
        ..Default::default()
    };
    let _ = am.insert(&st.db).await.map_err(internal)?;
    let _ = crate::util::login_check_and_mark(
        &key,
        st.cfg.login_max_attempts,
        st.cfg.login_window_secs,
        true,
    );
    Ok(Json(AuthLoginResp { token: token_str }))
}

pub async fn auth_proxy_token(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<AuthLoginResp>> {
    let Some(hname) = st.cfg.auth_proxy_header.as_ref() else {
        return Err(bad_request("proxy auth not configured"));
    };
    let name = axum::http::header::HeaderName::from_bytes(hname.as_bytes())
        .map_err(|_| bad_request("bad proxy header name"))?;
    let username = headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| unauthorized("missing proxy header"))?;
    let uid = if let Some(id) = find_user_id_by_username(&st.db, &username).await? {
        id
    } else if st.cfg.auth_proxy_user_creation {
        create_user_with_random_password(&st.db, &username).await?
    } else {
        return Err(unauthorized("user not found"));
    };
    let mut rand_bytes = [0u8; 32];
    rand_core::OsRng.fill_bytes(&mut rand_bytes);
    let token_str = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(rand_bytes);
    let token_hash = format!("{:x}", Sha256::digest(token_str.as_bytes()));
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let am = token::ActiveModel {
        user_id: Set(uid),
        name: Set(Some("proxy".into())),
        token_hash: Set(token_hash),
        token_plain: Set(Some(token_str.clone())),
        created_at: Set(now),
        last_used_at: Set(Some(now)),
        expires_at: Set(Some(
            now + chrono::Duration::seconds({
                let v: i64 = std::env::var("CAPTURA_TOKEN_TTL_SECS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(30 * 24 * 3600);
                v.max(3600)
            }),
        )),
        ..Default::default()
    };
    let _ = am.insert(&st.db).await.map_err(internal)?;
    Ok(Json(AuthLoginResp { token: token_str }))
}
