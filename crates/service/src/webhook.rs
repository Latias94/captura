use captura_common::UserId;
use captura_storage::entity::webhook;
use hmac::{Hmac, Mac};
use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QuerySelect, RelationTrait,
};
use sha2::Sha256;

pub async fn emit_new_entries(
    db: &DatabaseConnection,
    user_id: UserId,
    feed: &captura_storage::entity::feed::Model,
    entry_ids: &[i64],
) -> captura_common::Result<()> {
    if entry_ids.is_empty() {
        return Ok(());
    }
    let hooks = webhook::Entity::find()
        .filter(webhook::Column::UserId.eq(user_id.0))
        .filter(webhook::Column::Enabled.eq(true))
        .all(db)
        .await
        .map_err(|e| captura_common::Error::Storage(e.to_string()))?;
    if hooks.is_empty() {
        return Ok(());
    }
    use captura_storage::entity::{enclosure, entry, entry_label, label};
    let entries = entry::Entity::find()
        .filter(entry::Column::Id.is_in(entry_ids.to_vec()))
        .all(db)
        .await
        .map_err(|e| captura_common::Error::Storage(e.to_string()))?;
    // load enclosures per entry
    let encs = enclosure::Entity::find()
        .filter(enclosure::Column::EntryId.is_in(entry_ids.to_vec()))
        .all(db)
        .await
        .map_err(|e| captura_common::Error::Storage(e.to_string()))?;
    use std::collections::HashMap;
    let mut enc_map: HashMap<i64, Vec<serde_json::Value>> = HashMap::new();
    for e in encs {
        enc_map
            .entry(e.entry_id)
            .or_default()
            .push(serde_json::json!({
                "id": e.id,
                "user_id": user_id.0,
                "entry_id": e.entry_id,
                "url": e.url,
                "mime_type": e.mime.unwrap_or_default(),
                "size": e.length.unwrap_or(0),
                "media_progression": e.media_progression.unwrap_or(0),
            }));
    }
    // tags per entry
    let pairs: Vec<(i64, String)> = entry_label::Entity::find()
        .join(
            sea_orm::JoinType::InnerJoin,
            entry_label::Relation::Label.def(),
        )
        .filter(entry_label::Column::EntryId.is_in(entry_ids.to_vec()))
        .filter(label::Column::UserId.eq(user_id.0))
        .select_only()
        .column(entry_label::Column::EntryId)
        .column(label::Column::Name)
        .into_tuple()
        .all(db)
        .await
        .map_err(|e| captura_common::Error::Storage(e.to_string()))?;
    let mut tag_map: HashMap<i64, Vec<String>> = HashMap::new();
    for (eid, name) in pairs {
        tag_map.entry(eid).or_default().push(name);
    }

    let wpm = 200usize;
    let entries_json: Vec<serde_json::Value> = entries
        .into_iter()
        .map(|e| {
            let body = e
                .content_html
                .clone()
                .or(e.summary.clone())
                .unwrap_or_default();
            let text = ammonia::clean(&body);
            let words = text.split_whitespace().count();
            let reading_time = std::cmp::max(1, words.div_ceil(wpm)) as i32;
            serde_json::json!({
                "id": e.id,
                "user_id": user_id.0,
                "feed_id": e.feed_id,
                "status": if e.is_read { "read" } else { "unread" },
                "hash": e.hash,
                "title": e.title,
                "url": e.url,
                "comments_url": "",
                "published_at": e.published_at.map(|d| d.to_rfc3339()),
                "created_at": e.created_at.to_rfc3339(),
                "changed_at": e.updated_at.to_rfc3339(),
                "content": e.content_html,
                "share_code": "",
                "starred": e.is_starred,
                "reading_time": reading_time,
                "enclosures": enc_map.remove(&e.id).unwrap_or_default(),
                "tags": tag_map.remove(&e.id).unwrap_or_default(),
            })
        })
        .collect();

    let feed_json = serde_json::json!({
        "id": feed.id,
        "user_id": user_id.0,
        "feed_url": feed.feed_url,
        "site_url": feed.site_url,
        "title": feed.title,
        "checked_at": feed.checked_at.map(|d| d.to_rfc3339()),
    });
    let payload = serde_json::json!({
        "event_type": "new_entries",
        "feed": feed_json,
        "entries": entries_json,
    });
    deliver(db, user_id.0, "new_entries", &payload).await
}

pub async fn emit_save_entry(
    db: &DatabaseConnection,
    user_id: UserId,
    entry: &captura_storage::entity::entry::Model,
) -> captura_common::Result<()> {
    // load feed
    let feed = captura_storage::entity::feed::Entity::find_by_id(entry.feed_id)
        .one(db)
        .await
        .map_err(|e| captura_common::Error::Storage(e.to_string()))?;
    let feed_json = feed.map(|f| {
        serde_json::json!({
            "id": f.id,
            "user_id": user_id,
            "feed_url": f.feed_url,
            "site_url": f.site_url,
            "title": f.title,
            "checked_at": f.checked_at.map(|d| d.to_rfc3339()),
        })
    });
    let payload = serde_json::json!({
        "event_type": "save_entry",
        "entry": {
            "id": entry.id,
            "user_id": user_id,
            "feed_id": entry.feed_id,
            "status": if entry.is_read { "read" } else { "unread" },
            "hash": entry.hash,
            "title": entry.title,
            "url": entry.url,
            "comments_url": "",
            "published_at": entry.published_at.map(|d| d.to_rfc3339()),
            "created_at": entry.created_at.to_rfc3339(),
            "changed_at": entry.updated_at.to_rfc3339(),
            "content": entry.content_html,
            "author": entry.author,
            "share_code": "",
            "starred": entry.is_starred,
            "reading_time": entry
                .content_html
                .as_ref()
                .map(|html| captura_common::reading_time_minutes_from_html(html, 200))
                .unwrap_or(0),
            "enclosures": [],
            "tags": [],
            "feed": feed_json,
        }
    });
    deliver(db, user_id.0, "save_entry", &payload).await
}

async fn deliver(
    db: &DatabaseConnection,
    user_id: i64,
    event_type: &str,
    payload: &serde_json::Value,
) -> captura_common::Result<()> {
    let hooks = webhook::Entity::find()
        .filter(webhook::Column::UserId.eq(user_id))
        .filter(webhook::Column::Enabled.eq(true))
        .all(db)
        .await
        .map_err(|e| captura_common::Error::Storage(e.to_string()))?;
    if hooks.is_empty() {
        return Ok(());
    }
    let body = serde_json::to_vec(payload).unwrap_or_default();
    let cli = crate::http_client_basic()?;
    for h in hooks {
        if let Some(ref ev) = h.events {
            let mut allow = false;
            for part in ev.split(',') {
                if part.trim() == event_type {
                    allow = true;
                    break;
                }
            }
            if !allow {
                continue;
            }
        }
        // HMAC-SHA256 signature
        let mut mac = Hmac::<Sha256>::new_from_slice(h.secret.as_bytes())
            .map_err(|e| captura_common::Error::Other(anyhow::anyhow!(e)))?;
        mac.update(&body);
        let sig = hex::encode(mac.finalize().into_bytes());
        let req = cli
            .post(&h.url)
            .header("Content-Type", "application/json")
            .header("X-Miniflux-Signature", sig)
            .header("X-Miniflux-Event-Type", event_type)
            .body(body.clone());
        let _ = req.send().await; // minimal implementation: no retries
    }
    Ok(())
}
