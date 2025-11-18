use argon2::PasswordHasher;
use axum::extract::Path;
use axum::Json;
use axum_extra::typed_header::TypedHeader;
use chrono::{FixedOffset, Utc};
use headers::authorization::Bearer;
use headers::Authorization;
use md5::Md5;
use rand_core::OsRng;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, Set};
use serde::{Deserialize, Serialize};
use sha2::Digest;

use crate::auth::AuthUser;
use crate::error::{bad_request, forbidden, internal, ApiResult};
use crate::AppState;
use captura_storage::entity::{user, user_pref};

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
    let is_first = count == 0;
    if !is_first && !st.cfg.signup_enabled {
        return Err(forbidden("signup disabled"));
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
        role: Set(if is_first {
            captura_storage::entity::user::UserRole::Admin
        } else {
            captura_storage::entity::user::UserRole::User
        }),
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

#[derive(Serialize)]
pub struct MeDto {
    pub id: i64,
    pub username: String,
    pub is_admin: bool,
    pub theme: String,
    pub language: String,
    pub entries_per_page: i32,
    pub entry_sorting_direction: String,
    pub stylesheet: String,
    pub custom_js: String,
    pub external_font_hosts: String,
    pub keyboard_shortcuts: bool,
    pub show_reading_time: bool,
    pub open_external_links_in_new_tab: bool,
    pub mark_read_on_view: bool,
}

fn is_admin(role: &user::UserRole) -> bool {
    matches!(role, user::UserRole::Admin)
}

pub async fn me(
    axum::extract::State(st): axum::extract::State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
) -> ApiResult<Json<MeDto>> {
    let auth = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let Some(u) = user::Entity::find_by_id(auth.user_id)
        .one(&st.db)
        .await
        .map_err(internal)?
    else {
        return Err(crate::error::not_found("user"));
    };

    let prefs = user_pref::Entity::find()
        .filter(user_pref::Column::UserId.eq(u.id))
        .all(&st.db)
        .await
        .map_err(internal)?;

    let get_str = |k: &str| -> Option<String> {
        prefs
            .iter()
            .find(|p| p.key == k)
            .and_then(|p| p.value_json.as_ref())
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
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

    let mut theme = "system_serif".to_string();
    let mut language = "en_US".to_string();
    let mut entry_sorting_direction = "desc".to_string();
    let mut stylesheet = String::new();
    let mut custom_js_val = String::new();
    let mut external_font_hosts = String::new();
    let mut entries_per_page = 100;
    let mut keyboard_shortcuts = true;
    let mut show_reading_time = true;
    let mut open_external_links_in_new_tab = false;
    let mut mark_read_on_view = false;

    if let Some(s) = get_str("theme") {
        theme = s;
    }
    if let Some(s) = get_str("language") {
        language = s;
    }
    if let Some(s) = get_str("entry_sorting_direction") {
        entry_sorting_direction = s;
    }
    if let Some(s) = get_str("stylesheet") {
        stylesheet = s;
    }
    if let Some(s) = get_str("custom_js") {
        custom_js_val = s;
    }
    if let Some(s) = get_str("external_font_hosts") {
        external_font_hosts = s;
    }
    if let Some(n) = get_i("entries_per_page") {
        entries_per_page = n;
    }
    if let Some(b) = get_bool("keyboard_shortcuts") {
        keyboard_shortcuts = b;
    }
    if let Some(b) = get_bool("show_reading_time") {
        show_reading_time = b;
    }
    if let Some(b) = get_bool("open_external_links_in_new_tab") {
        open_external_links_in_new_tab = b;
    }
    if let Some(b) = get_bool("mark_read_on_view") {
        mark_read_on_view = b;
    }

    let dto = MeDto {
        id: u.id,
        username: u.username.clone(),
        is_admin: is_admin(&u.role),
        theme,
        language,
        entries_per_page,
        entry_sorting_direction,
        stylesheet,
        custom_js: custom_js_val,
        external_font_hosts,
        keyboard_shortcuts,
        show_reading_time,
        open_external_links_in_new_tab,
        mark_read_on_view,
    };
    Ok(Json(dto))
}

#[derive(Deserialize, Default)]
pub struct UserPrefsUpdateReq {
    pub theme: Option<String>,
    pub language: Option<String>,
    pub entries_per_page: Option<i32>,
    #[serde(default)]
    pub entry_sorting_direction: Option<String>,
    pub stylesheet: Option<String>,
    pub custom_js: Option<String>,
    pub external_font_hosts: Option<String>,
    pub keyboard_shortcuts: Option<bool>,
    pub show_reading_time: Option<bool>,
    pub open_external_links_in_new_tab: Option<bool>,
    pub mark_read_on_view: Option<bool>,
}

pub async fn update_prefs(
    axum::extract::State(st): axum::extract::State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Json(body): Json<UserPrefsUpdateReq>,
) -> ApiResult<&'static str> {
    let auth = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let user_id = auth.user_id;

    async fn upsert_pref(
        db: &sea_orm::DatabaseConnection,
        user_id: i64,
        key: &str,
        v: serde_json::Value,
    ) -> ApiResult<()> {
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
        upsert_pref(&st.db, user_id, "theme", serde_json::json!(s)).await?;
    }
    if let Some(s) = &body.language {
        upsert_pref(&st.db, user_id, "language", serde_json::json!(s)).await?;
    }
    if let Some(n) = body.entries_per_page {
        upsert_pref(
            &st.db,
            user_id,
            "entries_per_page",
            serde_json::json!(n),
        )
        .await?;
    }
    if let Some(s) = &body.entry_sorting_direction {
        upsert_pref(
            &st.db,
            user_id,
            "entry_sorting_direction",
            serde_json::json!(s),
        )
        .await?;
    }
    if let Some(s) = &body.stylesheet {
        upsert_pref(&st.db, user_id, "stylesheet", serde_json::json!(s)).await?;
    }
    if let Some(s) = &body.custom_js {
        upsert_pref(&st.db, user_id, "custom_js", serde_json::json!(s)).await?;
    }
    if let Some(s) = &body.external_font_hosts {
        upsert_pref(
            &st.db,
            user_id,
            "external_font_hosts",
            serde_json::json!(s),
        )
        .await?;
    }
    if let Some(b) = body.keyboard_shortcuts {
        upsert_pref(
            &st.db,
            user_id,
            "keyboard_shortcuts",
            serde_json::json!(b),
        )
        .await?;
    }
    if let Some(b) = body.show_reading_time {
        upsert_pref(
            &st.db,
            user_id,
            "show_reading_time",
            serde_json::json!(b),
        )
        .await?;
    }
    if let Some(b) = body.open_external_links_in_new_tab {
        upsert_pref(
            &st.db,
            user_id,
            "open_external_links_in_new_tab",
            serde_json::json!(b),
        )
        .await?;
    }
    if let Some(b) = body.mark_read_on_view {
        upsert_pref(
            &st.db,
            user_id,
            "mark_read_on_view",
            serde_json::json!(b),
        )
        .await?;
    }

    Ok("ok")
}
