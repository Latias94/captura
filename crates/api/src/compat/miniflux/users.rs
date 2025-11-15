use super::error::{bad_request, forbidden, from_api_error, internal, not_found, MfResult};
use crate::auth::mf_auth;
use crate::AppState;
use argon2::PasswordHasher;
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::Json;
use chrono::{FixedOffset, Utc};
use rand_core::OsRng;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, Set,
};

use captura_storage::entity::user as user_entity;
use captura_storage::entity::{entry, feed, user_pref};
use sea_orm::QuerySelect;

// ---------- DTO ----------
#[derive(serde::Serialize)]
pub(crate) struct MfUserFullDto {
    pub id: i64,
    pub username: String,
    pub is_admin: bool,
    pub theme: String,
    pub language: String,
    pub timezone: String,
    pub entry_sorting_direction: String,
    pub stylesheet: String,
    pub custom_js: String,
    pub external_font_hosts: String,
    pub google_id: String,
    pub openid_connect_id: String,
    pub entries_per_page: i32,
    pub keyboard_shortcuts: bool,
    pub show_reading_time: bool,
    pub entry_swipe: bool,
    pub always_open_external_links: bool,
    pub open_external_links_in_new_tab: bool,
    pub mark_read_on_view: bool,
    pub last_login_at: Option<String>,
}

#[derive(serde::Deserialize)]
pub(crate) struct MfCreateUserReq {
    pub username: String,
    pub password: String,
    pub is_admin: Option<bool>,
}

#[derive(serde::Deserialize, Default)]
pub(crate) struct MfUpdateUserReq {
    pub username: Option<String>,
    pub password: Option<String>,
    pub is_admin: Option<bool>,
    // preferences (optional)
    pub theme: Option<String>,
    pub language: Option<String>,
    pub timezone: Option<String>,
    pub entry_sorting_direction: Option<String>,
    pub stylesheet: Option<String>,
    pub custom_js: Option<String>,
    pub external_font_hosts: Option<String>,
    pub entries_per_page: Option<i32>,
    pub keyboard_shortcuts: Option<bool>,
    pub show_reading_time: Option<bool>,
    pub entry_swipe: Option<bool>,
    pub always_open_external_links: Option<bool>,
    pub open_external_links_in_new_tab: Option<bool>,
    pub mark_read_on_view: Option<bool>,
}

// ---------- helpers ----------
fn is_admin_role(role: &user_entity::UserRole) -> bool {
    matches!(role, user_entity::UserRole::Admin)
}

pub(super) async fn ensure_admin(st: &AppState, user_id: i64) -> MfResult<()> {
    let u = user_entity::Entity::find_by_id(user_id)
        .one(&st.db)
        .await
        .map_err(internal)?
        .ok_or_else(|| not_found("user"))?;
    if is_admin_role(&u.role) {
        Ok(())
    } else {
        Err(forbidden("admin required").into())
    }
}

fn default_user_full(u: &user_entity::Model) -> MfUserFullDto {
    MfUserFullDto {
        id: u.id,
        username: u.username.clone(),
        is_admin: is_admin_role(&u.role),
        theme: "system_serif".into(),
        language: "en_US".into(),
        timezone: "UTC".into(),
        entry_sorting_direction: "desc".into(),
        stylesheet: String::new(),
        custom_js: String::new(),
        external_font_hosts: String::new(),
        google_id: String::new(),
        openid_connect_id: String::new(),
        entries_per_page: 100,
        keyboard_shortcuts: true,
        show_reading_time: true,
        entry_swipe: true,
        always_open_external_links: false,
        open_external_links_in_new_tab: false,
        mark_read_on_view: false,
        last_login_at: None,
    }
}

async fn map_user_full_with_prefs(
    db: &sea_orm::DatabaseConnection,
    u: user_entity::Model,
) -> MfResult<MfUserFullDto> {
    let mut dto = default_user_full(&u);
    let prefs = user_pref::Entity::find()
        .filter(user_pref::Column::UserId.eq(u.id))
        .all(db)
        .await
        .map_err(internal)?;
    let get_str = |k: &str| -> Option<String> {
        prefs
            .iter()
            .find(|p| p.key == k)
            .and_then(|p| p.value_json.as_ref())
            .and_then(|v| v.as_str().map(|s| s.to_string()))
    };
    let get_bool = |k: &str| -> Option<bool> {
        prefs
            .iter()
            .find(|p| p.key == k)
            .and_then(|p| p.value_json.as_ref())
            .and_then(|v| v.as_bool())
    };
    let get_i = |k: &str| -> Option<i32> {
        prefs
            .iter()
            .find(|p| p.key == k)
            .and_then(|p| p.value_json.as_ref())
            .and_then(|v| v.as_i64())
            .map(|n| n as i32)
    };

    if let Some(s) = get_str("theme") {
        dto.theme = s;
    }
    if let Some(s) = get_str("language") {
        dto.language = s;
    }
    if let Some(s) = get_str("timezone") {
        dto.timezone = s;
    }
    if let Some(s) = get_str("entry_sorting_direction") {
        dto.entry_sorting_direction = s;
    }
    if let Some(s) = get_str("stylesheet") {
        dto.stylesheet = s;
    }
    if let Some(s) = get_str("custom_js") {
        dto.custom_js = s;
    }
    if let Some(s) = get_str("external_font_hosts") {
        dto.external_font_hosts = s;
    }
    if let Some(n) = get_i("entries_per_page") {
        dto.entries_per_page = n;
    }
    if let Some(b) = get_bool("keyboard_shortcuts") {
        dto.keyboard_shortcuts = b;
    }
    if let Some(b) = get_bool("show_reading_time") {
        dto.show_reading_time = b;
    }
    if let Some(b) = get_bool("entry_swipe") {
        dto.entry_swipe = b;
    }
    if let Some(b) = get_bool("always_open_external_links") {
        dto.always_open_external_links = b;
    }
    if let Some(b) = get_bool("open_external_links_in_new_tab") {
        dto.open_external_links_in_new_tab = b;
    }
    if let Some(b) = get_bool("mark_read_on_view") {
        dto.mark_read_on_view = b;
    }
    Ok(dto)
}

// ---------- handlers ----------
pub(crate) async fn me(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
) -> MfResult<Json<MfUserFullDto>> {
    let auth = mf_auth(&st, &headers).await.map_err(from_api_error)?;
    let u = user_entity::Entity::find_by_id(auth.user_id)
        .one(&st.db)
        .await
        .map_err(internal)?
        .ok_or_else(|| not_found("user"))?;
    let dto = map_user_full_with_prefs(&st.db, u).await?;
    Ok(Json(dto))
}

pub(crate) async fn list(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
) -> MfResult<Json<Vec<MfUserFullDto>>> {
    let auth = mf_auth(&st, &headers).await.map_err(from_api_error)?;
    ensure_admin(&st, auth.user_id).await?;
    let users = user_entity::Entity::find()
        .order_by_asc(user_entity::Column::Id)
        .all(&st.db)
        .await
        .map_err(internal)?;
    let mut out = Vec::new();
    for u in users {
        out.push(map_user_full_with_prefs(&st.db, u).await?);
    }
    Ok(Json(out))
}

pub(crate) async fn get(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id_or_name): Path<String>,
) -> MfResult<Json<MfUserFullDto>> {
    let auth = mf_auth(&st, &headers).await.map_err(from_api_error)?;
    ensure_admin(&st, auth.user_id).await?;
    let by_id = id_or_name.parse::<i64>().ok();
    let user = if let Some(id) = by_id {
        user_entity::Entity::find_by_id(id)
            .one(&st.db)
            .await
            .map_err(internal)?
    } else {
        user_entity::Entity::find()
            .filter(user_entity::Column::Username.eq(id_or_name))
            .one(&st.db)
            .await
            .map_err(internal)?
    };
    let u = user.ok_or_else(|| not_found("user"))?;
    Ok(Json(map_user_full_with_prefs(&st.db, u).await?))
}

pub(crate) async fn create(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(body): Json<MfCreateUserReq>,
) -> MfResult<Json<MfUserFullDto>> {
    let auth = mf_auth(&st, &headers).await.map_err(from_api_error)?;
    ensure_admin(&st, auth.user_id).await?;
    if body.username.trim().is_empty() || body.password.is_empty() {
        return Err(bad_request("username/password required").into());
    }
    let exists = user_entity::Entity::find()
        .filter(user_entity::Column::Username.eq(&body.username))
        .count(&st.db)
        .await
        .map_err(internal)?;
    if exists > 0 {
        return Err(bad_request("username exists").into());
    }
    let salt = argon2::password_hash::SaltString::generate(&mut OsRng);
    let hash = argon2::Argon2::default()
        .hash_password(body.password.as_bytes(), &salt)
        .map_err(internal)?
        .to_string();
    let role = if body.is_admin.unwrap_or(false) {
        user_entity::UserRole::Admin
    } else {
        user_entity::UserRole::User
    };
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let am = user_entity::ActiveModel {
        username: Set(body.username),
        password_hash: Set(hash),
        fever_key_md5: Set(None),
        role: Set(role),
        created_at: Set(now),
        ..Default::default()
    };
    let u = am.insert(&st.db).await.map_err(internal)?;
    Ok(Json(map_user_full_with_prefs(&st.db, u).await?))
}

pub(crate) async fn update(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<MfUpdateUserReq>,
) -> MfResult<Json<MfUserFullDto>> {
    let auth = mf_auth(&st, &headers).await.map_err(from_api_error)?;
    ensure_admin(&st, auth.user_id).await?;
    let Some(model) = user_entity::Entity::find_by_id(id)
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("user").into());
    };
    let mut am: user_entity::ActiveModel = model.into();
    if let Some(name) = body.username {
        am.username = Set(name);
    }
    if let Some(is_admin) = body.is_admin {
        am.role = Set(if is_admin {
            user_entity::UserRole::Admin
        } else {
            user_entity::UserRole::User
        });
    }
    if let Some(pw) = body.password {
        let salt = argon2::password_hash::SaltString::generate(&mut OsRng);
        let hash = argon2::Argon2::default()
            .hash_password(pw.as_bytes(), &salt)
            .map_err(internal)?
            .to_string();
        am.password_hash = Set(hash);
    }
    let u = am.update(&st.db).await.map_err(internal)?;

    async fn upsert_pref(
        db: &sea_orm::DatabaseConnection,
        user_id: i64,
        key: &str,
        v: serde_json::Value,
    ) -> MfResult<()> {
        let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
        if let Some(model) = user_pref::Entity::find()
            .filter(user_pref::Column::UserId.eq(user_id))
            .filter(user_pref::Column::Key.eq(key))
            .one(db)
            .await
            .map_err(internal)?
        {
            let mut am: user_pref::ActiveModel = model.into();
            am.value_json = Set(Some(v));
            am.updated_at = Set(now);
            let _ = am.update(db).await.map_err(internal)?;
        } else {
            let am = user_pref::ActiveModel {
                user_id: Set(user_id),
                key: Set(key.to_string()),
                value_json: Set(Some(v)),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            };
            let _ = am.insert(db).await.map_err(internal)?;
        }
        Ok(())
    }
    if let Some(s) = &body.theme {
        upsert_pref(&st.db, u.id, "theme", serde_json::json!(s)).await?;
    }
    if let Some(s) = &body.language {
        upsert_pref(&st.db, u.id, "language", serde_json::json!(s)).await?;
    }
    if let Some(s) = &body.timezone {
        upsert_pref(&st.db, u.id, "timezone", serde_json::json!(s)).await?;
    }
    if let Some(s) = &body.entry_sorting_direction {
        upsert_pref(
            &st.db,
            u.id,
            "entry_sorting_direction",
            serde_json::json!(s),
        )
        .await?;
    }
    if let Some(s) = &body.stylesheet {
        upsert_pref(&st.db, u.id, "stylesheet", serde_json::json!(s)).await?;
    }
    if let Some(s) = &body.custom_js {
        upsert_pref(&st.db, u.id, "custom_js", serde_json::json!(s)).await?;
    }
    if let Some(s) = &body.external_font_hosts {
        upsert_pref(&st.db, u.id, "external_font_hosts", serde_json::json!(s)).await?;
    }
    if let Some(n) = body.entries_per_page {
        upsert_pref(&st.db, u.id, "entries_per_page", serde_json::json!(n)).await?;
    }
    if let Some(b) = body.keyboard_shortcuts {
        upsert_pref(&st.db, u.id, "keyboard_shortcuts", serde_json::json!(b)).await?;
    }
    if let Some(b) = body.show_reading_time {
        upsert_pref(&st.db, u.id, "show_reading_time", serde_json::json!(b)).await?;
    }
    if let Some(b) = body.entry_swipe {
        upsert_pref(&st.db, u.id, "entry_swipe", serde_json::json!(b)).await?;
    }
    if let Some(b) = body.always_open_external_links {
        upsert_pref(
            &st.db,
            u.id,
            "always_open_external_links",
            serde_json::json!(b),
        )
        .await?;
    }
    if let Some(b) = body.open_external_links_in_new_tab {
        upsert_pref(
            &st.db,
            u.id,
            "open_external_links_in_new_tab",
            serde_json::json!(b),
        )
        .await?;
    }
    if let Some(b) = body.mark_read_on_view {
        upsert_pref(&st.db, u.id, "mark_read_on_view", serde_json::json!(b)).await?;
    }

    Ok(Json(map_user_full_with_prefs(&st.db, u).await?))
}

pub(crate) async fn delete(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
) -> MfResult<axum::response::Response> {
    let auth = mf_auth(&st, &headers).await.map_err(from_api_error)?;
    ensure_admin(&st, auth.user_id).await?;
    if id == auth.user_id {
        return Err(bad_request("cannot delete self").into());
    }
    let Some(u) = user_entity::Entity::find_by_id(id)
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(not_found("user").into());
    };
    let am: user_entity::ActiveModel = u.into();
    am.delete(&st.db).await.map_err(internal)?;
    Ok((
        axum::http::StatusCode::NO_CONTENT,
        axum::body::Body::empty(),
    )
        .into_response())
}

// Mark all entries of a given user as read (only allowed for self)
pub(crate) async fn mark_all_read(
    State(st): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
) -> MfResult<&'static str> {
    let auth = mf_auth(&st, &headers).await.map_err(from_api_error)?;
    if auth.user_id != id {
        return Err(not_found("user").into());
    }
    let feed_ids: Vec<i64> = feed::Entity::find()
        .filter(feed::Column::UserId.eq(id))
        .select_only()
        .column(feed::Column::Id)
        .into_tuple()
        .all(&st.db)
        .await
        .map_err(internal)?;
    if !feed_ids.is_empty() {
        let _ = entry::Entity::update_many()
            .col_expr(entry::Column::IsRead, sea_orm::sea_query::Expr::value(true))
            .filter(entry::Column::FeedId.is_in(feed_ids))
            .exec(&st.db)
            .await
            .map_err(internal)?;
    }
    Ok("ok")
}
