use argon2::PasswordHasher;
use axum::http::header::ACCEPT;
use axum::{
    extract::State,
    response::IntoResponse,
    response::{Html, Redirect, Response},
    Json,
};
use base64::Engine as _;
use chrono::{FixedOffset, Utc};
use hmac::{Hmac, Mac};
use openidconnect::core::*;
use openidconnect::reqwest::async_http_client;
use openidconnect::{
    AuthenticationFlow, AuthorizationCode, ClientId, ClientSecret, CsrfToken, IssuerUrl, Nonce,
    RedirectUrl, Scope,
};
use rand_core::{OsRng, RngCore};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::error::{bad_request, internal, unauthorized, ApiResult};
use crate::state::OidcProvider;
use crate::AppState;
use captura_storage::entity::{token, user};

fn hmac_sign(secret: &str, payload_b64: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("hmac");
    mac.update(payload_b64.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

fn hmac_verify(secret: &str, payload_b64: &str, sig: &str) -> bool {
    let expected = hmac_sign(secret, payload_b64);
    // Minimal implementation: regular comparison; for stronger side-channel resistance consider a constant-time compare
    expected == sig
}

pub(crate) async fn start(State(st): State<AppState>) -> ApiResult<impl IntoResponse> {
    let cfg = &st.cfg;
    if !cfg.oidc_enabled {
        return Err(bad_request("oidc disabled"));
    }
    let client = build_client_from_default(cfg).await?;

    // Prepare state (HMAC-signed), include nonce
    let mut nonce_bytes = [0u8; 16];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(nonce_bytes);
    let payload = serde_json::json!({"nonce": nonce});
    let payload_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(serde_json::to_vec(&payload).map_err(internal)?);
    let sig = hmac_sign(&cfg.oidc_state_secret, &payload_b64);
    let state_token = format!("{}.{}", payload_b64, sig);

    let nonce_clone = nonce.clone();
    let (auth_url, _csrf, _returned_nonce) = client
        .authorize_url(
            AuthenticationFlow::<CoreResponseType>::AuthorizationCode,
            || CsrfToken::new(state_token),
            move || Nonce::new(nonce_clone.clone()),
        )
        .add_scope(Scope::new("openid".into()))
        .add_scope(Scope::new("email".into()))
        // we pass Nonce via state payload and re-construct at callback for validation
        .url();

    Ok(Redirect::to(auth_url.as_ref()))
}

pub(crate) async fn start_named(
    State(st): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> ApiResult<impl IntoResponse> {
    let cfg = &st.cfg;
    let prov = cfg
        .oidc_providers
        .iter()
        .find(|p| p.name == name)
        .cloned()
        .ok_or_else(|| bad_request("unknown oidc provider"))?;
    let client = build_client(&prov).await?;
    // state + nonce
    let mut nonce_bytes = [0u8; 16];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(nonce_bytes);
    let payload = serde_json::json!({"nonce": nonce, "provider": prov.name});
    let payload_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(serde_json::to_vec(&payload).map_err(internal)?);
    let sig = hmac_sign(&cfg.oidc_state_secret, &payload_b64);
    let state_token = format!("{}.{}", payload_b64, sig);
    let nonce_clone = nonce.clone();
    let (auth_url, _csrf, _returned_nonce) = client
        .authorize_url(
            AuthenticationFlow::<CoreResponseType>::AuthorizationCode,
            || CsrfToken::new(state_token),
            move || Nonce::new(nonce_clone.clone()),
        )
        .add_scope(Scope::new("openid".into()))
        .add_scope(Scope::new("email".into()))
        .url();
    Ok(Redirect::to(auth_url.as_ref()))
}

#[derive(Deserialize)]
pub(crate) struct CallbackQuery {
    code: String,
    state: String,
}

#[derive(serde::Serialize)]
pub(crate) struct OidcLoginResp {
    token: String,
}

/// List configured OIDC provider names for the frontend to render login choices.
pub(crate) async fn oidc_providers(State(st): State<AppState>) -> ApiResult<Json<Vec<String>>> {
    let names: Vec<String> = st
        .cfg
        .oidc_providers
        .iter()
        .map(|p| p.name.clone())
        .collect();
    Ok(Json(names))
}

pub(crate) async fn callback(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    axum::extract::Query(q): axum::extract::Query<CallbackQuery>,
) -> ApiResult<Response> {
    let cfg = &st.cfg;
    if !cfg.oidc_enabled {
        return Err(bad_request("oidc disabled"));
    }
    // Parse state
    let (payload_b64, sig) = match q.state.split_once('.') {
        Some(p) => p,
        None => return Err(unauthorized("invalid state")),
    };
    if !hmac_verify(&cfg.oidc_state_secret, payload_b64, sig) {
        return Err(unauthorized("bad state signature"));
    }
    let payload_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload_b64.as_bytes())
        .map_err(|_| unauthorized("bad state encoding"))?;
    let json: serde_json::Value = serde_json::from_slice(&payload_bytes).map_err(internal)?;
    let nonce_str = json
        .get("nonce")
        .and_then(|v| v.as_str())
        .ok_or_else(|| unauthorized("missing nonce"))?;
    let expected_nonce = Nonce::new(nonce_str.to_string());

    let client = build_client_from_default(cfg).await?;

    // Exchange code -> tokens
    let token_resp = client
        .exchange_code(AuthorizationCode::new(q.code.clone()))
        .request_async(async_http_client)
        .await
        .map_err(internal)?;
    let id_token = token_resp
        .extra_fields()
        .id_token()
        .ok_or_else(|| unauthorized("missing id_token"))?;
    let claims = id_token
        .claims(&client.id_token_verifier(), &expected_nonce)
        .map_err(|_| unauthorized("invalid id_token"))?;

    // Map user
    let username = if let Some(email) = claims.email() {
        email.to_string()
    } else {
        format!("oidc:{}", claims.subject().as_str())
    };
    // Upsert user
    let existing = user::Entity::find()
        .filter(user::Column::Username.eq(&username))
        .one(&st.db)
        .await
        .map_err(internal)?;
    let u = if let Some(u) = existing {
        u
    } else {
        let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
        let mut rand_bytes = [0u8; 32];
        OsRng.fill_bytes(&mut rand_bytes);
        let rand_pw = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(rand_bytes);
        let salt = argon2::password_hash::SaltString::generate(&mut OsRng);
        let hash = argon2::Argon2::default()
            .hash_password(rand_pw.as_bytes(), &salt)
            .map_err(internal)?
            .to_string();
        let am = user::ActiveModel {
            username: Set(username.clone()),
            password_hash: Set(hash),
            fever_key_md5: Set(None),
            role: Set(captura_storage::entity::user::UserRole::User),
            created_at: Set(now),
            ..Default::default()
        };
        am.insert(&st.db).await.map_err(internal)?
    };

    // Issue API token
    let mut rand_bytes = [0u8; 32];
    OsRng.fill_bytes(&mut rand_bytes);
    let token_str = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(rand_bytes);
    let token_hash = format!("{:x}", Sha256::digest(token_str.as_bytes()));
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let am = token::ActiveModel {
        user_id: Set(u.id),
        name: Set(Some("oidc".into())),
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
    let want_html = headers
        .get(ACCEPT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.contains("text/html"))
        .unwrap_or(false)
        || matches!(
            headers.get("x-view").and_then(|v| v.to_str().ok()),
            Some("html")
        );
    if want_html {
        let html = format!(
            "<html><head><meta charset=\"utf-8\"><title>Captura OIDC</title></head><body>\
            <h3>登录成功</h3><p>请复制以下 API Token：</p>\
            <textarea style=\"width:100%;height:6em;\" readonly>{}</textarea>\
            <p>使用方法：在请求头中携带 <code>X-Auth-Token</code> 即可。</p>\
            </body></html>",
            token_str
        );
        Ok(Html(html).into_response())
    } else {
        Ok(Json(OidcLoginResp { token: token_str }).into_response())
    }
}

pub(crate) async fn callback_named(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    axum::extract::Path(name): axum::extract::Path<String>,
    axum::extract::Query(q): axum::extract::Query<CallbackQuery>,
) -> ApiResult<Response> {
    let cfg = &st.cfg;
    let (payload_b64, sig) = match q.state.split_once('.') {
        Some(p) => p,
        None => return Err(unauthorized("invalid state")),
    };
    if !hmac_verify(&cfg.oidc_state_secret, payload_b64, sig) {
        return Err(unauthorized("bad state signature"));
    }
    let payload_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload_b64.as_bytes())
        .map_err(|_| unauthorized("bad state encoding"))?;
    let json: serde_json::Value = serde_json::from_slice(&payload_bytes).map_err(internal)?;
    // allow state provider override if present
    let prov_name = json
        .get("provider")
        .and_then(|v| v.as_str())
        .unwrap_or(&name);
    let prov = cfg
        .oidc_providers
        .iter()
        .find(|p| p.name == prov_name)
        .cloned()
        .ok_or_else(|| bad_request("unknown oidc provider"))?;
    let nonce_str = json
        .get("nonce")
        .and_then(|v| v.as_str())
        .ok_or_else(|| unauthorized("missing nonce"))?;
    let expected_nonce = Nonce::new(nonce_str.to_string());
    let client = build_client(&prov).await?;
    // Exchange code
    let token_resp = client
        .exchange_code(AuthorizationCode::new(q.code.clone()))
        .request_async(async_http_client)
        .await
        .map_err(internal)?;
    let id_token = token_resp
        .extra_fields()
        .id_token()
        .ok_or_else(|| unauthorized("missing id_token"))?;
    let claims = id_token
        .claims(&client.id_token_verifier(), &expected_nonce)
        .map_err(|_| unauthorized("invalid id_token"))?;
    // Map user + issue token (reuse logic via helper)
    finish_with_user_and_token(&st, headers, claims).await
}

async fn build_client_from_default(cfg: &crate::state::AppConfig) -> ApiResult<CoreClient> {
    let issuer = IssuerUrl::new(cfg.oidc_issuer_url.clone()).map_err(internal)?;
    let md = CoreProviderMetadata::discover_async(issuer, async_http_client)
        .await
        .map_err(internal)?;
    let client = CoreClient::from_provider_metadata(
        md,
        ClientId::new(cfg.oidc_client_id.clone()),
        Some(ClientSecret::new(cfg.oidc_client_secret.clone())),
    )
    .set_redirect_uri(RedirectUrl::new(cfg.oidc_redirect_url.clone()).map_err(internal)?);
    Ok(client)
}

async fn build_client(p: &OidcProvider) -> ApiResult<CoreClient> {
    let issuer = IssuerUrl::new(p.issuer_url.clone()).map_err(internal)?;
    let md = CoreProviderMetadata::discover_async(issuer, async_http_client)
        .await
        .map_err(internal)?;
    let client = CoreClient::from_provider_metadata(
        md,
        ClientId::new(p.client_id.clone()),
        Some(ClientSecret::new(p.client_secret.clone())),
    )
    .set_redirect_uri(RedirectUrl::new(p.redirect_url.clone()).map_err(internal)?);
    Ok(client)
}

async fn finish_with_user_and_token(
    st: &AppState,
    headers: axum::http::HeaderMap,
    claims: &CoreIdTokenClaims,
) -> ApiResult<Response> {
    let username = if let Some(email) = claims.email() {
        email.to_string()
    } else {
        format!("oidc:{}", claims.subject().as_str())
    };
    let existing = user::Entity::find()
        .filter(user::Column::Username.eq(&username))
        .one(&st.db)
        .await
        .map_err(internal)?;
    let u = if let Some(u) = existing {
        u
    } else {
        let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
        let mut rand_bytes = [0u8; 32];
        OsRng.fill_bytes(&mut rand_bytes);
        let rand_pw = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(rand_bytes);
        let salt = argon2::password_hash::SaltString::generate(&mut OsRng);
        let hash = argon2::Argon2::default()
            .hash_password(rand_pw.as_bytes(), &salt)
            .map_err(internal)?
            .to_string();
        let am = user::ActiveModel {
            username: Set(username.clone()),
            password_hash: Set(hash),
            fever_key_md5: Set(None),
            role: Set(captura_storage::entity::user::UserRole::User),
            created_at: Set(now),
            ..Default::default()
        };
        am.insert(&st.db).await.map_err(internal)?
    };
    // Issue token
    let mut rand_bytes = [0u8; 32];
    OsRng.fill_bytes(&mut rand_bytes);
    let token_str = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(rand_bytes);
    let token_hash = format!("{:x}", Sha256::digest(token_str.as_bytes()));
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let am = token::ActiveModel {
        user_id: Set(u.id),
        name: Set(Some("oidc".into())),
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
    let want_html = headers
        .get(ACCEPT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.contains("text/html"))
        .unwrap_or(false)
        || matches!(
            headers.get("x-view").and_then(|v| v.to_str().ok()),
            Some("html")
        );
    if want_html {
        let html = format!(
            "<html><head><meta charset=\"utf-8\"><title>Captura OIDC</title></head><body>\
            <h3>登录成功</h3><p>请复制以下 API Token：</p>\
            <textarea style=\"width:100%;height:6em;\" readonly>{}</textarea>\
            <p>使用方法：在请求头中携带 <code>X-Auth-Token</code> 即可。</p>\
            </body></html>",
            token_str
        );
        Ok(axum::response::Html(html).into_response())
    } else {
        Ok(Json(OidcLoginResp { token: token_str }).into_response())
    }
}
