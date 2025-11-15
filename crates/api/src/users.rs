use argon2::PasswordHasher;
use axum::extract::Path;
use axum::Json;
use axum_extra::typed_header::TypedHeader;
use chrono::{FixedOffset, Utc};
use headers::authorization::Bearer;
use headers::Authorization;
use md5::Md5;
use rand_core::OsRng;
use sea_orm::{ActiveModelTrait, EntityTrait, PaginatorTrait, Set};
use serde::{Deserialize, Serialize};
use sha2::Digest;

use crate::auth::AuthUser;
use crate::error::{bad_request, forbidden, internal, ApiResult};
use crate::AppState;
use captura_storage::entity::user;

#[derive(Deserialize)]
pub struct CreateUserReq {
    pub username: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct CreateUserResp {
    pub id: i64,
}

pub async fn create_user(
    axum::extract::State(st): axum::extract::State<AppState>,
    Json(body): Json<CreateUserReq>,
) -> ApiResult<Json<CreateUserResp>> {
    let count = user::Entity::find().count(&st.db).await.map_err(internal)?;
    if count > 0 {
        return Err(forbidden("user exists"));
    }
    let salt = argon2::password_hash::SaltString::generate(&mut OsRng);
    let hash = argon2::Argon2::default()
        .hash_password(body.password.as_bytes(), &salt)
        .map_err(internal)?
        .to_string();
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let am = user::ActiveModel {
        username: Set(body.username),
        password_hash: Set(hash),
        role: Set(captura_storage::entity::user::UserRole::Admin),
        created_at: Set(now),
        ..Default::default()
    };
    let u = am.insert(&st.db).await.map_err(internal)?;
    Ok(Json(CreateUserResp { id: u.id }))
}

#[derive(Deserialize)]
pub struct SetFeverKeyReq {
    pub api_password: String,
}

pub async fn set_fever_key(
    axum::extract::State(st): axum::extract::State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Path(id): Path<i64>,
    Json(req): Json<SetFeverKeyReq>,
) -> ApiResult<&'static str> {
    let auth = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    if auth.user_id != id {
        return Err(forbidden("cannot set fever key for other user"));
    }
    if req.api_password.trim().is_empty() {
        return Err(bad_request("api_password required"));
    }
    let Some(u) = user::Entity::find_by_id(id)
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(crate::error::not_found("user not found"));
    };
    let s = format!("{}:{}", u.username, req.api_password);
    let key = format!("{:x}", Md5::digest(s.as_bytes()));
    let mut am: user::ActiveModel = u.into();
    am.fever_key_md5 = Set(Some(key));
    am.update(&st.db).await.map_err(internal)?;
    Ok("ok")
}
