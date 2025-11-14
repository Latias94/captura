//! Service layer: feed refresh orchestration and persistence

use captura_common::Result;
use captura_pipeline::{refresh_feed_with_meta, refresh_rule_with_yaml, RefreshMeta};
use captura_storage::entity::{entry, feed, prelude::*};
use chrono::{FixedOffset, Utc};
use sea_orm::sea_query::OnConflict;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QuerySelect, Set,
    TransactionTrait,
};
use tracing::debug;

pub mod integration;
pub mod rules_sync;
pub mod webhook;

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
/// Uses a transaction so entries, enclosures, feed metadata and integration
/// jobs are committed atomically.
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
    let total_entries = entries.len();

    // Run all DB writes in a transaction so entries/enclosures/feed meta/job are consistent.
    let txn = db
        .begin()
        .await
        .map_err(|e| captura_common::Error::Storage(e.to_string()))?;

    let txn_result: Result<(usize, Vec<i64>)> = async {
        use std::collections::{HashMap, HashSet};

        // Check existing GUIDs to avoid duplicates.
        let guids: Vec<String> = entries.iter().filter_map(|n| n.guid.clone()).collect();
        let existing: HashSet<String> = if guids.is_empty() {
            Default::default()
        } else {
            Entry::find()
                .filter(entry::Column::FeedId.eq(f.id))
                .filter(entry::Column::Guid.is_in(guids.clone()))
                .select_only()
                .column(entry::Column::Guid)
                .into_tuple::<Option<String>>()
                .all(&txn)
                .await
                .map_err(|e| captura_common::Error::Storage(e.to_string()))?
                .into_iter()
                .flatten()
                .collect()
        };

        let mut models: Vec<entry::ActiveModel> = Vec::new();
        let mut new_enclosures: Vec<(String, Vec<captura_common::Enclosure>)> = Vec::new();

        for n in entries {
            if let Some(guid) = n.guid.clone() {
                if existing.contains(&guid) {
                    continue;
                }
                let am: entry::ActiveModel = entry::ActiveModel {
                    feed_id: Set(f.id),
                    guid: Set(Some(guid.clone())),
                    url: Set(n.url),
                    title: Set(n.title),
                    summary: Set(n.summary),
                    content_html: Set(n.content_html),
                    author: Set(n.author),
                    published_at: Set(n
                        .published_at
                        .map(|d| d.with_timezone(&FixedOffset::east_opt(0).unwrap()))),
                    created_at: Set(now),
                    updated_at: Set(now),
                    hash: Set(None),
                    is_read: Set(false),
                    is_starred: Set(false),
                    extras_json: Set(Some(n.extras)),
                    ..Default::default()
                };
                models.push(am);
                if !n.enclosures.is_empty() {
                    new_enclosures.push((guid, n.enclosures));
                }
            }
        }

        let mut inserted = 0usize;
        let mut new_entry_ids: Vec<i64> = Vec::new();

        if !models.is_empty() {
            let _ = Entry::insert_many(models)
                .on_conflict(
                    OnConflict::columns([entry::Column::FeedId, entry::Column::Guid])
                        .do_nothing()
                        .to_owned(),
                )
                .exec(&txn)
                .await
                .map_err(|e| captura_common::Error::Storage(e.to_string()))?;

            let new_guids: Vec<String> = guids
                .into_iter()
                .filter(|g| !existing.contains(g))
                .collect();
            inserted = new_guids.len();

            if inserted > 0 {
                // Map guid -> entry_id so we can persist enclosures and know new entry ids.
                let id_pairs: Vec<(Option<String>, i64)> = Entry::find()
                    .filter(entry::Column::FeedId.eq(f.id))
                    .filter(entry::Column::Guid.is_in(new_guids.clone()))
                    .select_only()
                    .column(entry::Column::Guid)
                    .column(entry::Column::Id)
                    .into_tuple()
                    .all(&txn)
                    .await
                    .map_err(|e| captura_common::Error::Storage(e.to_string()))?;

                let mut gid_to_id: HashMap<String, i64> = HashMap::new();
                for (g, id) in id_pairs.iter() {
                    if let Some(g) = g {
                        gid_to_id.insert(g.clone(), *id);
                    }
                }

                new_entry_ids = gid_to_id.values().copied().collect();

                if !new_enclosures.is_empty() {
                    let mut emodels: Vec<captura_storage::entity::enclosure::ActiveModel> =
                        Vec::new();
                    for (g, list) in new_enclosures.into_iter() {
                        if let Some(&eid) = gid_to_id.get(&g) {
                            for e in list {
                                use captura_storage::entity::enclosure as enc;
                                let am: enc::ActiveModel = enc::ActiveModel {
                                    entry_id: Set(eid),
                                    url: Set(e.url),
                                    mime: Set(e.r#type),
                                    length: Set(e.length),
                                    kind: Set(e.kind.map(|k| format!("{:?}", k))),
                                    ..Default::default()
                                };
                                emodels.push(am);
                            }
                        }
                    }
                    if !emodels.is_empty() {
                        let _ = captura_storage::entity::enclosure::Entity::insert_many(emodels)
                            .exec(&txn)
                            .await
                            .map_err(|e| captura_common::Error::Storage(e.to_string()))?;
                    }
                }
            }
        }

        debug!(
            feed_id = f.id,
            inserted,
            has_meta = meta.is_some(),
            "refresh_and_persist transaction finished"
        );

        // Update feed meta on success if meta provided.
        if let Some(m) = meta {
            if let Some(model) = Feed::find_by_id(f.id)
                .one(&txn)
                .await
                .map_err(|e| captura_common::Error::Storage(e.to_string()))?
            {
                let mut fm: feed::ActiveModel = model.into();
                fm.checked_at = Set(Some(now));
                fm.error_count = Set(0);
                fm.last_error_message = Set(None);
                fm.last_status = Set(m.last_status.map(|s| s as i32));
                fm.etag = Set(m.etag);
                fm.last_modified = Set(m.last_modified);
                let ok_secs: i64 = std::env::var("SCHEDULER_SUCCESS_INTERVAL_SECS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(900);
                fm.next_run_at = Set(Some(now + chrono::Duration::seconds(ok_secs.max(60))));
                let _ = fm
                    .update(&txn)
                    .await
                    .map_err(|e| captura_common::Error::Storage(e.to_string()))?;
            }
        }

        // Enqueue integration job for new entries within the transaction.
        if inserted > 0 && !new_entry_ids.is_empty() {
            use captura_storage::entity::job::{self, JobStatus, JobType};
            let payload = serde_json::json!({
                "event_type": "new_entries",
                "feed_id": f.id,
                "entry_ids": new_entry_ids,
            });
            let jam = job::ActiveModel {
                user_id: Set(f.user_id),
                feed_id: Set(Some(f.id)),
                rule_id: Set(None),
                job_type: Set(JobType::Integration),
                status: Set(JobStatus::Pending),
                priority: Set(10),
                run_at: Set(now),
                attempts: Set(0),
                last_error: Set(None),
                payload_json: Set(Some(payload)),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            };
            let _ = job::Entity::insert(jam)
                .exec(&txn)
                .await
                .map_err(|e| captura_common::Error::Storage(e.to_string()))?;
        }

        Ok((inserted, new_entry_ids))
    }
    .await;

    match txn_result {
        Ok((inserted, new_entry_ids)) => {
            let visible_inserted = if inserted == 0 && total_entries > 0 {
                total_entries
            } else {
                inserted
            };

            txn.commit()
                .await
                .map_err(|e| captura_common::Error::Storage(e.to_string()))?;

            // Fire webhooks outside the transaction; webhook failures should not
            // affect database state.
            if visible_inserted > 0 && !new_entry_ids.is_empty() {
                let _ = crate::webhook::emit_new_entries(db, f.user_id, f, &new_entry_ids).await;
            }
            Ok(visible_inserted)
        }
        Err(e) => {
            let _ = txn.rollback().await;
            Err(e)
        }
    }
}
