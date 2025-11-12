use axum::http::HeaderMap;
use chrono::{FixedOffset, Utc};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use sha2::{Digest, Sha256};

use argon2::PasswordVerifier;
use base64::Engine;
use captura_storage::entity::{prelude::*, token};

use crate::error::{internal, unauthorized, ApiResult};
use crate::AppState;

#[derive(Clone)]
pub struct AuthUser {
    pub user_id: i64,
}

impl AuthUser {
    pub async fn from_bearer(db: &DatabaseConnection, bearer: &str) -> ApiResult<Self> {
        let hash = format!("{:x}", Sha256::digest(bearer.as_bytes()));
        let tok = Token::find()
            .filter(token::Column::TokenHash.eq(hash))
            .one(db)
            .await
            .map_err(internal)?;
        let Some(tok) = tok else {
            return Err(unauthorized("invalid token"));
        };
        let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
        if let Some(exp) = tok.expires_at {
            if exp <= now {
                return Err(unauthorized("token expired"));
            }
        }
        let mut am: token::ActiveModel = tok.clone().into();
        am.last_used_at = Set(Some(now));
        let _ = am.update(db).await;
        Ok(Self {
            user_id: tok.user_id,
        })
    }
}

/// Miniflux/兼容层通用鉴权：优先 X-Auth-Token；回退 Authorization: Bearer
// legacy Miniflux/Fever 兼容鉴权已不再使用
#[allow(dead_code)]
pub async fn mf_auth(st: &AppState, headers: &HeaderMap) -> ApiResult<AuthUser> {
    if let Some(v) = headers.get("X-Auth-Token") {
        if let Ok(token) = v.to_str() {
            return AuthUser::from_bearer(&st.db, token).await;
        }
    }
    if let Some(v) = headers.get(axum::http::header::AUTHORIZATION) {
        if let Ok(s) = v.to_str() {
            if let Some(t) = s.strip_prefix("Bearer ") {
                return AuthUser::from_bearer(&st.db, t).await;
            }
            // 兼容 Basic 鉴权（Miniflux 支持 Basic 与 Token）：Authorization: Basic base64(username:password)
            if let Some(b64) = s.strip_prefix("Basic ") {
                if let Ok(raw) = base64::engine::general_purpose::STANDARD.decode(b64) {
                    if let Ok(pair) = std::str::from_utf8(&raw) {
                        if let Some((username, password)) = pair.split_once(':') {
                            // 校验用户口令
                            let u = User::find()
                                .filter(
                                    captura_storage::entity::user::Column::Username.eq(username),
                                )
                                .one(&st.db)
                                .await
                                .map_err(internal)?;
                            if let Some(u) = u {
                                if let Ok(parsed) = argon2::PasswordHash::new(&u.password_hash) {
                                    if argon2::Argon2::default()
                                        .verify_password(password.as_bytes(), &parsed)
                                        .is_ok()
                                    {
                                        return Ok(AuthUser { user_id: u.id });
                                    }
                                }
                            }
                            return Err(unauthorized("invalid credentials"));
                        }
                    }
                }
                return Err(unauthorized("invalid basic header"));
            }
        }
    }
    Err(unauthorized("missing token"))
}
