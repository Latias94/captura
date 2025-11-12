use axum::http::HeaderMap;
use chrono::{FixedOffset, Utc};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use sha2::{Digest, Sha256};

use argon2::{PasswordHasher, PasswordVerifier};
use base64::Engine;
use captura_storage::entity::{prelude::*, token};

use crate::error::{internal, unauthorized, ApiResult};
use crate::AppState;
use argon2::password_hash::SaltString;
use rand_core::{OsRng, RngCore};

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
            if !st.cfg.disable_local_auth {
                if let Some(b64) = s.strip_prefix("Basic ") {
                    if let Ok(raw) = base64::engine::general_purpose::STANDARD.decode(b64) {
                        if let Ok(pair) = std::str::from_utf8(&raw) {
                            if let Some((username, password)) = pair.split_once(':') {
                                // 校验用户口令
                                let u = User::find()
                                    .filter(
                                        captura_storage::entity::user::Column::Username
                                            .eq(username),
                                    )
                                    .one(&st.db)
                                    .await
                                    .map_err(internal)?;
                                if let Some(u) = u {
                                    if let Ok(parsed) = argon2::PasswordHash::new(&u.password_hash)
                                    {
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
    }
    // 反向代理认证（基于受信任的请求头）
    if let Some(ref hdr) = st.cfg.auth_proxy_header {
        if !hdr.is_empty() {
            let name = axum::http::header::HeaderName::from_bytes(hdr.as_bytes())
                .map_err(|_| internal("bad proxy header name"))?;
            if let Some(v) = headers.get(name) {
                if let Ok(username) = v.to_str() {
                    if !username.trim().is_empty() {
                        if let Some(uid) = find_user_id_by_username(&st.db, username).await? {
                            return Ok(AuthUser { user_id: uid });
                        }
                        if st.cfg.auth_proxy_user_creation {
                            let uid = create_user_with_random_password(&st.db, username).await?;
                            return Ok(AuthUser { user_id: uid });
                        }
                    }
                }
            }
        }
    }
    Err(unauthorized("missing token"))
}

pub(crate) async fn find_user_id_by_username(
    db: &DatabaseConnection,
    username: &str,
) -> ApiResult<Option<i64>> {
    let u = User::find()
        .filter(captura_storage::entity::user::Column::Username.eq(username))
        .one(db)
        .await
        .map_err(internal)?;
    Ok(u.map(|m| m.id))
}

pub(crate) async fn create_user_with_random_password(
    db: &DatabaseConnection,
    username: &str,
) -> ApiResult<i64> {
    // use captura_storage::entity::prelude::*;
    use captura_storage::entity::user as user_entity;
    use chrono::{FixedOffset, Utc};
    if username.trim().is_empty() {
        return Err(unauthorized("invalid proxy username"));
    }
    if let Some(id) = find_user_id_by_username(db, username).await? {
        return Ok(id);
    }
    let salt = SaltString::generate(&mut OsRng);
    let mut buf = [0u8; 32];
    OsRng.fill_bytes(&mut buf);
    let rand_pw = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf);
    let hash = argon2::Argon2::default()
        .hash_password(rand_pw.as_bytes(), &salt)
        .map_err(internal)?
        .to_string();
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let am = user_entity::ActiveModel {
        username: Set(username.to_string()),
        password_hash: Set(hash),
        fever_key_md5: Set(None),
        role: Set(user_entity::UserRole::User),
        created_at: Set(now),
        ..Default::default()
    };
    let rec = am.insert(db).await.map_err(internal)?;
    Ok(rec.id)
}
