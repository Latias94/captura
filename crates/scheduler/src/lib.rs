//! Job scheduling and background workers.

use captura_common::Result;
use captura_pipeline::{refresh_feed_with_meta, refresh_rule_with_yaml};
use captura_storage::entity::{entry, favicon as fv, feed, job, prelude::*, rule};
use chrono::{FixedOffset, Utc};
use reqwest::{Client, Url};
use sea_orm::PaginatorTrait;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect, Set,
};
use sea_orm::sea_query::OnConflict;
use std::env;
use tracing::{error, info, instrument};

#[derive(Clone, Debug)]
pub struct SchedulerConfig {
    pub concurrency: usize,
}

#[derive(Debug)]
pub struct Scheduler {
    cfg: SchedulerConfig,
}

impl Scheduler {
    pub fn new(cfg: SchedulerConfig) -> Self {
        Self { cfg }
    }

    #[instrument]
    pub async fn run(&self) -> Result<()> {
        info!(workers = self.cfg.concurrency, "scheduler started");
        Ok(())
    }
}

pub async fn run_once(db: &DatabaseConnection, max: u64) -> Result<usize> {
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let jobs = Job::find()
        .filter(job::Column::Status.eq(job::JobStatus::Pending))
        .filter(job::Column::RunAt.lte(now))
        .order_by_desc(job::Column::Priority)
        .order_by_asc(job::Column::RunAt)
        .limit(max)
        .all(db)
        .await
        .map_err(|e| captura_common::Error::Storage(e.to_string()))?;
    let concurrency: usize = env::var("SCHEDULER_WORKER_CONCURRENCY")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4);
    let per_host: usize = env::var("SCHEDULER_PER_HOST_CONCURRENCY")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2);

    use std::collections::HashMap;
    let mut per_host_count: HashMap<String, usize> = HashMap::new();
    let mut scheduled: Vec<job::Model> = Vec::new();
    for j in jobs {
        // Only gate per-host for feed refresh
        if matches!(j.job_type, job::JobType::FeedRefresh) {
            if let Some(fid) = j.feed_id {
                if let Some(f) = Feed::find_by_id(fid)
                    .one(db)
                    .await
                    .map_err(|e| captura_common::Error::Storage(e.to_string()))?
                {
                    let host = reqwest::Url::parse(&f.feed_url)
                        .ok()
                        .and_then(|u| u.host_str().map(|s| s.to_string()))
                        .unwrap_or_else(|| "".into());
                    let cnt = per_host_count.entry(host.clone()).or_insert(0);
                    if *cnt >= per_host {
                        continue; // skip scheduling this time
                    }
                    *cnt += 1;
                }
            }
        }
        scheduled.push(j);
        if scheduled.len() >= concurrency { break; }
    }

    use futures::stream::{FuturesUnordered, StreamExt};
    let mut tasks = FuturesUnordered::new();
    for j in scheduled {
        let db = db.clone();
        let now = now;
        tasks.push(async move {
            // mark running
            if let Some(model) = Job::find_by_id(j.id).one(&db).await.ok().flatten() {
                let mut am: job::ActiveModel = model.into();
                am.status = Set(job::JobStatus::Running);
                am.attempts = Set(j.attempts + 1);
                am.updated_at = Set(now);
                let _ = am.update(&db).await;
            }
            let res = match j.job_type {
                job::JobType::FeedRefresh => refresh_feed_job(&db, &j).await,
                job::JobType::Favicon => refresh_favicon_job(&db, &j).await,
                _ => Err(captura_common::Error::Other(anyhow::anyhow!(
                    "unknown job type"
                ))),
            };
            // finalize
            if let Some(model) = Job::find_by_id(j.id).one(&db).await.ok().flatten() {
                let mut am: job::ActiveModel = model.into();
                match res {
                    Ok(_) => {
                        am.status = Set(job::JobStatus::Done);
                        am.last_error = Set(None);
                    }
                    Err(err) => {
                        am.status = Set(job::JobStatus::Failed);
                        am.last_error = Set(Some(err.to_string()));
                        if let Some(fid) = j.feed_id {
                            let _ = update_feed_on_failure(&db, fid, j.attempts).await;
                        }
                    }
                }
                am.updated_at = Set(Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap()));
                let _ = am.update(&db).await;
            }
            ()
        });
    }
    let mut processed = 0usize;
    while tasks.next().await.is_some() {
        processed += 1;
    }
    Ok(processed)
}

async fn refresh_feed_job(db: &DatabaseConnection, j: &job::Model) -> Result<()> {
    let Some(f) = Feed::find_by_id(j.feed_id.unwrap_or_default())
        .one(db)
        .await
        .map_err(|e| captura_common::Error::Storage(e.to_string()))?
    else {
        return Err(captura_common::Error::NotFound("feed missing".into()));
    };
    let (entries, meta) = if matches!(f.r#type, feed::FeedType::Rule) {
        let rule_yaml = match f.rule_id {
            Some(rid) => {
                let r = Rule::find_by_id(rid)
                    .one(db)
                    .await
                    .map_err(|e| captura_common::Error::Storage(e.to_string()))?
                    .ok_or_else(|| captura_common::Error::NotFound("rule missing".into()))?;
                r.yaml
            }
            None => {
                return Err(captura_common::Error::Config(
                    "rule_id required for rule-type feed".into(),
                ))
            }
        };
        (refresh_rule_with_yaml(&f, &rule_yaml).await?, None)
    } else {
        refresh_feed_with_meta(&f).await?
    };
    // insert entries
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let guids: Vec<String> = entries
        .iter()
        .filter_map(|n| n.guid.clone())
        .collect();
    let existing: std::collections::HashSet<String> = if guids.is_empty() {
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
            if existing.contains(&guid) { continue; }
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
    }
    // update feed schedule on success
    let mut fm: feed::ActiveModel = f.into();
    fm.checked_at = Set(Some(now));
    fm.error_count = Set(0);
    if let Some(m) = meta {
        fm.last_status = Set(m.last_status.map(|s| s as i32));
        fm.etag = Set(m.etag);
        fm.last_modified = Set(m.last_modified);
    }
    let ok_secs: i64 = env::var("SCHEDULER_SUCCESS_INTERVAL_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(900);
    fm.next_run_at = Set(Some(now + chrono::Duration::seconds(ok_secs.max(60))));
    let _ = fm
        .update(db)
        .await
        .map_err(|e| captura_common::Error::Storage(e.to_string()))?;
    Ok(())
}

async fn refresh_favicon_job(db: &DatabaseConnection, j: &job::Model) -> Result<()> {
    let Some(f) = Feed::find_by_id(j.feed_id.unwrap_or_default())
        .one(db)
        .await
        .map_err(|e| captura_common::Error::Storage(e.to_string()))?
    else {
        return Err(captura_common::Error::NotFound("feed missing".into()));
    };
    let site = f.site_url.clone().unwrap_or(f.feed_url.clone());
    let mut base = Url::parse(&site).map_err(|e| captura_common::Error::Config(e.to_string()))?;
    base.set_path("/favicon.ico");
    base.set_query(None);
    base.set_fragment(None);
    let cli = Client::builder()
        .user_agent("captura/0.1")
        .build()
        .map_err(|e| captura_common::Error::Network(e.to_string()))?;
    let res = cli
        .get(base.as_str())
        .send()
        .await
        .map_err(|e| captura_common::Error::Network(e.to_string()))?;
    if !res.status().is_success() {
        return Err(captura_common::Error::NotFound(format!(
            "status {}",
            res.status()
        )));
    }
    let mime = res
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let bytes = res
        .bytes()
        .await
        .map_err(|e| captura_common::Error::Network(e.to_string()))?
        .to_vec();
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let am = fv::ActiveModel {
        feed_id: Set(Some(f.id)),
        url: Set(Some(base.to_string())),
        mime: Set(mime),
        data: Set(Some(bytes)),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    let fav = am
        .insert(db)
        .await
        .map_err(|e| captura_common::Error::Storage(e.to_string()))?;
    let mut fm: feed::ActiveModel = f.into();
    fm.favicon_id = Set(Some(fav.id));
    let _ = fm
        .update(db)
        .await
        .map_err(|e| captura_common::Error::Storage(e.to_string()))?;
    Ok(())
}

#[instrument]
pub async fn enqueue_due_feeds(db: &DatabaseConnection, max: u64) -> Result<u64> {
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    // find due feeds
    let feeds = Feed::find()
        .filter(feed::Column::Disabled.eq(false))
        .filter(feed::Column::NextRunAt.lte(now))
        .order_by_asc(feed::Column::NextRunAt)
        .limit(max)
        .all(db)
        .await
        .map_err(|e| captura_common::Error::Storage(e.to_string()))?;
    let mut enq = 0u64;
    for f in feeds {
        // skip if there is already a pending/running refresh for this feed
        let exists = Job::find()
            .filter(job::Column::FeedId.eq(f.id))
            .filter(job::Column::JobType.eq(job::JobType::FeedRefresh))
            .filter(
                job::Column::Status.is_in(vec![job::JobStatus::Pending, job::JobStatus::Running]),
            )
            .count(db)
            .await
            .map_err(|e| captura_common::Error::Storage(e.to_string()))?;
        if exists > 0 {
            continue;
        }
        let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
        let am = job::ActiveModel {
            user_id: Set(f.user_id),
            feed_id: Set(Some(f.id)),
            rule_id: Set(None),
            job_type: Set(job::JobType::FeedRefresh),
            status: Set(job::JobStatus::Pending),
            priority: Set(0),
            run_at: Set(now),
            attempts: Set(0),
            last_error: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        };
        let _ = am
            .insert(db)
            .await
            .map_err(|e| captura_common::Error::Storage(e.to_string()))?;
        enq += 1;
    }
    Ok(enq)
}

async fn update_feed_on_failure(
    db: &DatabaseConnection,
    feed_id: i64,
    attempts: i32,
) -> Result<()> {
    let Some(f) = Feed::find_by_id(feed_id)
        .one(db)
        .await
        .map_err(|e| captura_common::Error::Storage(e.to_string()))?
    else {
        return Ok(());
    };
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let base: i64 = env::var("SCHEDULER_BACKOFF_BASE_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(300);
    let maxs: i64 = env::var("SCHEDULER_BACKOFF_MAX_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3600);
    let pow = (attempts as u32).min(10);
    // compute 2^pow safely
    let factor = if pow >= 63 {
        i64::MAX / base.max(1)
    } else {
        (1i64) << pow
    };
    let mut delay = base.saturating_mul(factor);
    if delay > maxs {
        delay = maxs;
    }
    let mut fm: feed::ActiveModel = f.into();
    fm.error_count = Set(attempts);
    fm.next_run_at = Set(Some(now + chrono::Duration::seconds(delay.max(60))));
    fm.updated_at = Set(now);
    let _ = fm
        .update(db)
        .await
        .map_err(|e| captura_common::Error::Storage(e.to_string()))?;
    Ok(())
}

// TODO: 添加 scheduler 集成测试（需可注入 fetcher/crawler mock）。
#[cfg(test)]
mod live_tests {
    use super::*;
    use migration::migrate;
    use sea_orm::PaginatorTrait;

    fn should_run_live() -> bool {
        std::env::var("CAPTURA_TEST_LIVE")
            .ok()
            .map(|v| v == "1" || v.to_lowercase() == "true")
            .unwrap_or(false)
    }

    async fn setup_db() -> DatabaseConnection {
        let db = captura_storage::connect("sqlite::memory:").await.unwrap();
        migrate(&db).await.unwrap();
        db
    }

    #[tokio::test]
    #[ignore]
    async fn enqueue_and_run_once_live_rust_blog() {
        if !should_run_live() {
            eprintln!("skip live test");
            return;
        }
        let db = setup_db().await;
        let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
        // user
        let u = captura_storage::entity::user::ActiveModel {
            username: Set("u".into()),
            password_hash: Set("h".into()),
            created_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
        // feed due now
        let f = captura_storage::entity::feed::ActiveModel {
            user_id: Set(u.id),
            category_id: Set(None),
            r#type: Set(captura_storage::entity::feed::FeedType::Atom),
            title: Set(Some("rust blog".into())),
            site_url: Set(Some("https://blog.rust-lang.org".into())),
            feed_url: Set("https://blog.rust-lang.org/feed.xml".into()),
            rule_id: Set(None),
            user_agent: Set(Some("captura-tests/0.1".into())),
            headers_json: Set(None),
            cookies: Set(None),
            proxy_url: Set(None),
            fetch_via_proxy: Set(false),
            disable_http2: Set(false),
            allow_invalid_certs: Set(false),
            request_timeout_ms: Set(Some(15000)),
            checked_at: Set(None),
            next_run_at: Set(Some(now - chrono::Duration::minutes(1))),
            etag: Set(None),
            last_modified: Set(None),
            last_status: Set(None),
            error_count: Set(0),
            disabled: Set(false),
            scraper_rules: Set(None),
            rewrite_rules: Set(None),
            blocklist_rules: Set(None),
            keeplist_rules: Set(None),
            url_rewrite_rules: Set(None),
            block_filter_entry_rules: Set(None),
            keep_filter_entry_rules: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            favicon_id: Set(None),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();

        let enq = enqueue_due_feeds(&db, 100).await.unwrap();
        assert!(enq >= 1);
        let processed = run_once(&db, 10).await.unwrap();
        assert!(processed >= 1);

        // entries persisted
        let cnt = Entry::find()
            .filter(entry::Column::FeedId.eq(f.id))
            .count(&db)
            .await
            .unwrap();
        assert!(cnt > 0, "should insert entries into DB");

        // job status done
        let j = Job::find()
            .order_by_desc(job::Column::Id)
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        assert!(
            matches!(j.status, job::JobStatus::Done) || matches!(j.status, job::JobStatus::Failed)
        );
    }
}
