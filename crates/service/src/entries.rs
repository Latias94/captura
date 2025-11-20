//! Entry-level operations (content extraction, saved flag, tags).
//!
//! This module centralizes business logic around a single entry so
//! that HTTP layers can remain thin adapters.

use captura_common::{Result, UserId};
use captura_pipeline::extractor;
use captura_storage::entity::{entry, entry_label, feed, label};
use captura_types::EntryContentDto;
use chrono::{FixedOffset, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QuerySelect, Set,
    TransactionTrait,
};

/// Fetch and optionally persist full content for an entry.
pub async fn get_entry_content(
    db: &DatabaseConnection,
    e: &entry::Model,
    update_content: bool,
) -> Result<EntryContentDto> {
    let Some(f) = feed::Entity::find_by_id(e.feed_id)
        .one(db)
        .await
        .map_err(|er| captura_common::Error::Storage(er.to_string()))?
    else {
        return Err(captura_common::Error::NotFound("feed".into()));
    };

    let page_url = match e.url.as_deref() {
        Some(u) => u,
        None => {
            let content = e
                .content_html
                .clone()
                .unwrap_or_else(|| e.summary.clone().unwrap_or_default());
            return Ok(EntryContentDto {
                content_html: content,
                title: e.title.clone(),
            });
        }
    };

    let extracted = extractor::fetch_and_extract_entry(page_url, &f).await?;
    let mut out_html = extracted.content_html.clone();
    let new_title = extracted.title;
    if out_html.is_empty() {
        let fallback = e
            .content_html
            .clone()
            .unwrap_or_else(|| e.summary.clone().unwrap_or_default());
        out_html = fallback;
    }

    if update_content {
        if let Some(model) = entry::Entity::find_by_id(e.id)
            .one(db)
            .await
            .map_err(|er| captura_common::Error::Storage(er.to_string()))?
        {
            let mut am: entry::ActiveModel = model.into();
            am.content_html = Set(Some(out_html.clone()));
            if let Some(nt) = new_title.clone() {
                am.title = Set(Some(nt));
            }
            am.updated_at = Set(Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap()));
            let _ = am
                .update(db)
                .await
                .map_err(|er| captura_common::Error::Storage(er.to_string()))?;
        }
    }

    Ok(EntryContentDto {
        content_html: out_html,
        title: new_title.or(e.title.clone()),
    })
}

/// Set or clear the "saved" flag on an entry, returning the updated model.
pub async fn set_entry_saved(
    db: &DatabaseConnection,
    e: &entry::Model,
    value: bool,
) -> Result<entry::Model> {
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let mut am: entry::ActiveModel = e.clone().into();
    if value {
        let saved_at = now.to_rfc3339();
        let extras = serde_json::json!({"saved": true, "saved_at": saved_at});
        am.extras_json = Set(Some(extras));
    } else {
        am.extras_json = Set(None);
    }
    am.updated_at = Set(now);
    let updated = am
        .update(db)
        .await
        .map_err(|er| captura_common::Error::Storage(er.to_string()))?;
    Ok(updated)
}

/// Add tags (labels) to an entry for a given user.
pub async fn add_tags_to_entry(
    db: &DatabaseConnection,
    user_id: UserId,
    e: &entry::Model,
    tags: Vec<String>,
) -> Result<()> {
    let mut names: Vec<String> = tags
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    names.sort();
    names.dedup();
    if names.is_empty() {
        return Ok(());
    }

    let txn = db
        .begin()
        .await
        .map_err(|er| captura_common::Error::Storage(er.to_string()))?;

    let existing: Vec<(i64, String)> = label::Entity::find()
        .filter(label::Column::UserId.eq(user_id.0))
        .filter(label::Column::Name.is_in(names.clone()))
        .select_only()
        .column(label::Column::Id)
        .column(label::Column::Name)
        .into_tuple()
        .all(&txn)
        .await
        .map_err(|er| captura_common::Error::Storage(er.to_string()))?;
    let mut name_to_id: std::collections::HashMap<String, i64> =
        existing.into_iter().map(|(id, n)| (n, id)).collect();

    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let missing: Vec<String> = names
        .iter()
        .filter(|n| !name_to_id.contains_key(*n))
        .cloned()
        .collect();
    for n in missing {
        let am = label::ActiveModel {
            id: Default::default(),
            user_id: Set(user_id.0),
            name: Set(n.clone()),
            color: Set(None),
            created_at: Set(now),
        };
        let l = am
            .insert(&txn)
            .await
            .map_err(|er| captura_common::Error::Storage(er.to_string()))?;
        name_to_id.insert(n, l.id);
    }

    let label_ids: Vec<i64> = names
        .iter()
        .filter_map(|n| name_to_id.get(n).copied())
        .collect();
    if !label_ids.is_empty() {
        let existing_pairs: Vec<i64> = entry_label::Entity::find()
            .filter(entry_label::Column::EntryId.eq(e.id))
            .filter(entry_label::Column::LabelId.is_in(label_ids.clone()))
            .select_only()
            .column(entry_label::Column::LabelId)
            .into_tuple()
            .all(&txn)
            .await
            .map_err(|er| captura_common::Error::Storage(er.to_string()))?;
        let exist_set: std::collections::HashSet<i64> = existing_pairs.into_iter().collect();
        for lid in label_ids.into_iter().filter(|lid| !exist_set.contains(lid)) {
            let am = entry_label::ActiveModel {
                entry_id: Set(e.id),
                label_id: Set(lid),
                ..Default::default()
            };
            let _ = am
                .insert(&txn)
                .await
                .map_err(|er| captura_common::Error::Storage(er.to_string()))?;
        }
    }

    txn.commit()
        .await
        .map_err(|er| captura_common::Error::Storage(er.to_string()))?;
    Ok(())
}

/// Remove tags (labels) from an entry for a given user.
pub async fn remove_tags_from_entry(
    db: &DatabaseConnection,
    user_id: UserId,
    e: &entry::Model,
    tags: Vec<String>,
) -> Result<()> {
    let mut names: Vec<String> = tags
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    names.sort();
    names.dedup();
    if names.is_empty() {
        return Ok(());
    }

    let txn = db
        .begin()
        .await
        .map_err(|er| captura_common::Error::Storage(er.to_string()))?;

    let label_ids: Vec<i64> = label::Entity::find()
        .filter(label::Column::UserId.eq(user_id.0))
        .filter(label::Column::Name.is_in(names))
        .select_only()
        .column(label::Column::Id)
        .into_tuple()
        .all(&txn)
        .await
        .map_err(|er| captura_common::Error::Storage(er.to_string()))?;
    if !label_ids.is_empty() {
        let _ = entry_label::Entity::delete_many()
            .filter(entry_label::Column::EntryId.eq(e.id))
            .filter(entry_label::Column::LabelId.is_in(label_ids))
            .exec(&txn)
            .await
            .map_err(|er| captura_common::Error::Storage(er.to_string()))?;
    }

    txn.commit()
        .await
        .map_err(|er| captura_common::Error::Storage(er.to_string()))?;
    Ok(())
}
