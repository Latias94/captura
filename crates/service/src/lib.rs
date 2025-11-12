//! Service layer: feed refresh orchestration and persistence

use captura_common::Result;
use captura_pipeline::{refresh_feed_with_meta, refresh_rule_with_yaml, RefreshMeta};
use captura_storage::entity::{entry, feed, prelude::*};
use chrono::{FixedOffset, Utc};
use sea_orm::sea_query::OnConflict;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QuerySelect, Set,
};

/// Refresh a feed by id and persist new entries, update feed metadata.
/// Returns number of inserted entries.
pub async fn refresh_and_persist_by_id(db: &DatabaseConnection, feed_id: i64) -> Result<usize> {
    let Some(f) = Feed::find_by_id(feed_id)
        .one(db)
        .await
        .map_err(|e| captura_common::Error::Storage(e.to_string()))?
    else {
        return Err(captura_common::Error::NotFound("feed not found".into()));
    };
    refresh_and_persist(db, &f).await
}

/// Refresh a feed model and persist new entries, update feed metadata.
/// Returns number of inserted entries.
pub async fn refresh_and_persist(db: &DatabaseConnection, f: &feed::Model) -> Result<usize> {
    let (entries, meta): (Vec<captura_common::NormalizedEntry>, Option<RefreshMeta>) =
        if matches!(f.r#type, feed::FeedType::Rule) {
            let yaml = if let Some(rid) = f.rule_id {
                let r = Rule::find_by_id(rid)
                    .one(db)
                    .await
                    .map_err(|e| captura_common::Error::Storage(e.to_string()))?
                    .ok_or_else(|| captura_common::Error::Config("rule missing".into()))?;
                r.yaml
            } else {
                return Err(captura_common::Error::Config(
                    "rule_id required for rule-type feed".into(),
                ));
            };
            (refresh_rule_with_yaml(f, &yaml).await?, None)
        } else {
            refresh_feed_with_meta(f).await?
        };

    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    // check existing GUIDs to avoid duplicates
    let guids: Vec<String> = entries.iter().filter_map(|n| n.guid.clone()).collect();
    use std::collections::HashSet;
    let existing: HashSet<String> = if guids.is_empty() {
        Default::default()
    } else {
        Entry::find()
            .filter(entry::Column::FeedId.eq(f.id))
            .filter(entry::Column::Guid.is_in(guids.clone()))
            .select_only()
            .column(entry::Column::Guid)
            .into_tuple::<Option<String>>()
            .all(db)
            .await
            .map_err(|e| captura_common::Error::Storage(e.to_string()))?
            .into_iter()
            .flatten()
            .collect()
    };

    let mut models: Vec<entry::ActiveModel> = Vec::new();
    for n in entries {
        if let Some(guid) = n.guid.clone() {
            if existing.contains(&guid) {
                continue;
            }
            let mut am: entry::ActiveModel = Default::default();
            am.feed_id = Set(f.id);
            am.guid = Set(Some(guid));
            am.url = Set(n.url);
            am.title = Set(n.title);
            am.summary = Set(n.summary);
            am.content_html = Set(n.content_html);
            am.author = Set(n.author);
            am.published_at = Set(n
                .published_at
                .map(|d| d.with_timezone(&FixedOffset::east_opt(0).unwrap())));
            am.created_at = Set(now);
            am.updated_at = Set(now);
            am.hash = Set(None);
            am.is_read = Set(false);
            am.is_starred = Set(false);
            am.extras_json = Set(Some(n.extras));
            models.push(am);
        }
    }
    let mut inserted = 0usize;
    if !models.is_empty() {
        let _ = Entry::insert_many(models)
            .on_conflict(
                OnConflict::columns([entry::Column::FeedId, entry::Column::Guid])
                    .do_nothing()
                    .to_owned(),
            )
            .exec(db)
            .await
            .map_err(|e| captura_common::Error::Storage(e.to_string()))?;
        inserted = guids.len().saturating_sub(existing.len());
    }

    // Update feed meta on success if meta provided
    if let Some(m) = meta {
        if let Some(model) = Feed::find_by_id(f.id)
            .one(db)
            .await
            .map_err(|e| captura_common::Error::Storage(e.to_string()))?
        {
            let mut fm: feed::ActiveModel = model.into();
            fm.checked_at = Set(Some(now));
            fm.error_count = Set(0);
            fm.last_status = Set(m.last_status.map(|s| s as i32));
            fm.etag = Set(m.etag);
            fm.last_modified = Set(m.last_modified);
            let ok_secs: i64 = std::env::var("SCHEDULER_SUCCESS_INTERVAL_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(900);
            fm.next_run_at = Set(Some(now + chrono::Duration::seconds(ok_secs.max(60))));
            let _ = fm
                .update(db)
                .await
                .map_err(|e| captura_common::Error::Storage(e.to_string()))?;
        }
    }

    Ok(inserted)
}
