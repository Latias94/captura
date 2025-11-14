//! Service layer: feed refresh orchestration and persistence

use captura_common::Result;
use captura_pipeline::{refresh_feed_with_meta, refresh_rule_with_yaml, RefreshMeta};
use captura_storage::entity::{entry, feed, prelude::*};
use chrono::{FixedOffset, Utc};
use sea_orm::sea_query::OnConflict;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QuerySelect, Set,
};
pub mod integration;
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
    // 记录新插入条目的 guid -> enclosures，用于落表
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

        // 插入 enclosures（仅对新插入的 guid）
        if !new_enclosures.is_empty() {
            // 映射 guid -> entry_id
            let new_guids: Vec<String> = new_enclosures.iter().map(|(g, _)| g.clone()).collect();
            let id_pairs: Vec<(Option<String>, i64)> = Entry::find()
                .filter(entry::Column::FeedId.eq(f.id))
                .filter(entry::Column::Guid.is_in(new_guids.clone()))
                .select_only()
                .column(entry::Column::Guid)
                .column(entry::Column::Id)
                .into_tuple()
                .all(db)
                .await
                .map_err(|e| captura_common::Error::Storage(e.to_string()))?;
            use std::collections::HashMap;
            let mut gid_to_id: HashMap<String, i64> = HashMap::new();
            for (g, id) in id_pairs {
                if let Some(g) = g {
                    gid_to_id.insert(g, id);
                }
            }
            // 构建插入模型
            let mut emodels: Vec<captura_storage::entity::enclosure::ActiveModel> = Vec::new();
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
                    .exec(db)
                    .await
                    .map_err(|e| captura_common::Error::Storage(e.to_string()))?;
            }
        }
    }

    // 触发 webhook: new_entries（仅当有新增）
    if inserted > 0 {
        // 查询新插入的 entry ids（根据 guids 差集）
        let new_guids: Vec<String> = guids
            .into_iter()
            .filter(|g| !existing.contains(g))
            .collect();
        if !new_guids.is_empty() {
            let ids: Vec<i64> = Entry::find()
                .filter(entry::Column::FeedId.eq(f.id))
                .filter(entry::Column::Guid.is_in(new_guids))
                .select_only()
                .column(entry::Column::Id)
                .into_tuple()
                .all(db)
                .await
                .map_err(|e| captura_common::Error::Storage(e.to_string()))?;
            let _ = crate::webhook::emit_new_entries(db, f.user_id, f, &ids).await;
            // 集成任务入队（避免跨 crate 依赖导致循环，引入直接落表实现）
            let payload = serde_json::json!({
                "event_type": "new_entries",
                "feed_id": f.id,
                "entry_ids": ids,
            });
            use captura_storage::entity::job::{self, JobStatus, JobType};
            let now2 = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
            let jam = job::ActiveModel {
                user_id: Set(f.user_id),
                feed_id: Set(Some(f.id)),
                rule_id: Set(None),
                job_type: Set(JobType::Integration),
                status: Set(JobStatus::Pending),
                priority: Set(10),
                run_at: Set(now2),
                attempts: Set(0),
                last_error: Set(None),
                payload_json: Set(Some(payload)),
                created_at: Set(now2),
                updated_at: Set(now2),
                ..Default::default()
            };
            let _ = job::Entity::insert(jam).exec(db).await;
        }
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
                .update(db)
                .await
                .map_err(|e| captura_common::Error::Storage(e.to_string()))?;
        }
    }

    Ok(inserted)
}
