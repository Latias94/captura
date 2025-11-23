use once_cell::sync::Lazy;
use sea_orm::DatabaseConnection;
use std::collections::HashMap;
use std::sync::Mutex;

use captura_storage::entity::{category, entry};
use captura_types::EntryDto;
use sea_orm::EntityTrait;

use crate::error::{ApiResult, bad_request, forbidden, internal};

// Common paging and sorting validation helpers
#[allow(dead_code)]
pub(crate) fn validate_limit_offset(limit: Option<u64>, _offset: Option<u64>) -> ApiResult<()> {
    if let Some(l) = limit {
        if l > 500 {
            return Err(bad_request("limit too large (max 500)"));
        }
    }
    Ok(())
}

#[allow(dead_code)]
pub(crate) fn validate_sort(
    sort_by: &Option<String>,
    allowed: &[&str],
    order: &Option<String>,
) -> ApiResult<()> {
    if let Some(s) = sort_by {
        if !allowed.iter().any(|a| a == s) {
            return Err(bad_request("invalid sort_by"));
        }
    }
    if let Some(o) = order {
        if o != "asc" && o != "desc" {
            return Err(bad_request("invalid order"));
        }
    }
    Ok(())
}

/// Map an `entry::Model` plus optional tag names into an `EntryDto`.
pub(crate) fn map_entry_to_dto(e: entry::Model, tags: Option<Vec<String>>) -> EntryDto {
    EntryDto {
        id: e.id,
        feed_id: e.feed_id,
        url: e.url,
        title: e.title,
        summary: e.summary,
        content_html: e.content_html,
        author: e.author,
        published_at: e.published_at.map(|d| d.to_rfc3339()),
        is_read: e.is_read,
        is_starred: e.is_starred,
        tags,
    }
}

#[allow(dead_code)]
pub(crate) async fn assert_category_ownership(
    db: &DatabaseConnection,
    user_id: i64,
    category_id: i64,
) -> ApiResult<()> {
    let cat = category::Entity::find_by_id(category_id)
        .one(db)
        .await
        .map_err(internal)?;
    let Some(cat) = cat else {
        return Err(bad_request("category not found"));
    };
    if cat.user_id != user_id {
        return Err(forbidden("category not owned by user"));
    }
    Ok(())
}

// Simple in-process login rate limiter keyed by username
static LOGIN_LIMITER: Lazy<Mutex<HashMap<String, (u32, std::time::Instant)>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub(crate) fn login_check_and_mark(
    key: &str,
    max: u32,
    window_secs: u64,
    success: bool,
) -> Result<(), &'static str> {
    let mut guard = LOGIN_LIMITER.lock().unwrap();
    let now = std::time::Instant::now();
    let ent = guard.entry(key.to_string()).or_insert((0, now));
    if now.duration_since(ent.1).as_secs() >= window_secs {
        ent.0 = 0;
        ent.1 = now;
    }
    if success {
        ent.0 = 0;
        ent.1 = now;
        return Ok(());
    }
    if ent.0 >= max {
        return Err("too_many_attempts");
    }
    ent.0 += 1;
    Ok(())
}
