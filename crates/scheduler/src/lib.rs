//! Job scheduling and background workers.

use captura_common::{FeedId, IntegrationEvent, Result, UserId};
// use captura_pipeline::{refresh_feed_with_meta, refresh_rule_with_yaml};
use captura_service as service;
use captura_storage::entity::{favicon as fv, feed, job};
use chrono::{FixedOffset, Utc};
use reqwest::Url;
use sea_orm::PaginatorTrait;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect, Set,
};
// use sea_orm::sea_query::OnConflict; // no longer needed after service extraction
use std::env;
use tracing::{info, instrument};

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
    let jobs = job::Entity::find()
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
    let per_user_limit: usize = env::var("SCHEDULER_PER_USER_CONCURRENCY")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(concurrency);

    use std::collections::HashMap;
    let mut per_host_count: HashMap<String, usize> = HashMap::new();
    let mut per_user_count: HashMap<i64, usize> = HashMap::new();
    let mut scheduled: Vec<job::Model> = Vec::new();
    for j in jobs {
        // Per-user concurrency gating across this run_once batch.
        let uid = j.user_id;
        let current_user_count = per_user_count.get(&uid).copied().unwrap_or(0);
        if current_user_count >= per_user_limit {
            continue;
        }

        // Only gate per-host for feed refresh
        if matches!(j.job_type, job::JobType::FeedRefresh) {
            if let Some(fid) = j.feed_id {
                if let Some(f) = feed::Entity::find_by_id(fid)
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
        // Passed per-user and per-host gates; schedule this job.
        per_user_count.insert(uid, current_user_count + 1);
        scheduled.push(j);
        if scheduled.len() >= concurrency {
            break;
        }
    }

    use futures::stream::{FuturesUnordered, StreamExt};
    let mut tasks = FuturesUnordered::new();
    for j in scheduled {
        let db = db.clone();
        tasks.push(async move {
            // mark running
            if let Some(model) = job::Entity::find_by_id(j.id).one(&db).await.ok().flatten() {
                let mut am: job::ActiveModel = model.into();
                am.status = Set(job::JobStatus::Running);
                am.attempts = Set(j.attempts + 1);
                am.updated_at = Set(now);
                let _ = am.update(&db).await;
            }
            let res = match j.job_type {
                job::JobType::FeedRefresh => refresh_feed_job(&db, &j).await,
                job::JobType::Favicon => refresh_favicon_job(&db, &j).await,
                job::JobType::Integration => deliver_integration_job(&db, &j).await,
                _ => Err(captura_common::Error::Other(anyhow::anyhow!(
                    "unknown job type"
                ))),
            };
            // finalize
            if let Some(model) = job::Entity::find_by_id(j.id).one(&db).await.ok().flatten() {
                let mut am: job::ActiveModel = model.into();
                match res {
                    Ok(_) => {
                        am.status = Set(job::JobStatus::Done);
                        am.last_error = Set(None);
                    }
                    Err(err) => {
                        let msg = err.to_string();
                        am.status = Set(job::JobStatus::Failed);
                        am.last_error = Set(Some(msg.clone()));
                        if let Some(fid) = j.feed_id {
                            // attempts 在运行前已 +1，这里应传入最新的 attempts 值
                            let _ = update_feed_on_failure(
                                &db,
                                FeedId(fid),
                                j.attempts + 1,
                                Some(msg),
                            )
                            .await;
                        } else if matches!(j.job_type, job::JobType::Integration) {
                            // 对于集成任务，按通用回退规则设置下一次运行时间
                            let now2 = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
                            let base: i64 = std::env::var("SCHEDULER_BACKOFF_BASE_SECS")
                                .ok()
                                .and_then(|s| s.parse().ok())
                                .unwrap_or(60);
                            let maxs: i64 = std::env::var("SCHEDULER_BACKOFF_MAX_SECS")
                                .ok()
                                .and_then(|s| s.parse().ok())
                                .unwrap_or(1800);
                            let pow = ((j.attempts + 1) as u32).min(10);
                            let factor = if pow >= 63 {
                                i64::MAX / base.max(1)
                            } else {
                                (1i64) << pow
                            };
                            let mut delay = base.saturating_mul(factor);
                            if delay > maxs {
                                delay = maxs;
                            }
                            am.run_at = Set(now2 + chrono::Duration::seconds(delay.max(30)));
                        }
                    }
                }
                am.updated_at = Set(Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap()));
                let _ = am.update(&db).await;
            }
        });
    }
    let mut processed = 0usize;
    while tasks.next().await.is_some() {
        processed += 1;
    }
    Ok(processed)
}

async fn refresh_feed_job(db: &DatabaseConnection, j: &job::Model) -> Result<()> {
    let fid = j.feed_id.unwrap_or_default();
    if fid == 0 {
        return Err(captura_common::Error::Config("job missing feed_id".into()));
    }
    let _ = service::refresh_and_persist_by_id(db, FeedId(fid)).await?;
    Ok(())
}

async fn refresh_favicon_job(db: &DatabaseConnection, j: &job::Model) -> Result<()> {
    let Some(f) = feed::Entity::find_by_id(j.feed_id.unwrap_or_default())
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
    let cli = captura_service::http_client_basic()?;
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
async fn deliver_integration_job(db: &DatabaseConnection, j: &job::Model) -> Result<()> {
    use captura_storage::entity::{entry, feed};
    let payload = j
        .payload_json
        .clone()
        .ok_or_else(|| captura_common::Error::Config("integration job missing payload".into()))?;
    let ev: IntegrationEvent =
        serde_json::from_value(payload).map_err(|e| captura_common::Error::Parse(e.to_string()))?;

    match ev {
        IntegrationEvent::NewEntries { feed_id, entry_ids } => {
            let f = feed::Entity::find_by_id(feed_id)
                .one(db)
                .await
                .map_err(|e| captura_common::Error::Storage(e.to_string()))?
                .ok_or_else(|| captura_common::Error::NotFound("feed".into()))?;
            captura_service::integration::emit_new_entries(db, j.user_id, &f, &entry_ids).await;
            Ok(())
        }
        IntegrationEvent::SaveEntry { entry_id, .. } => {
            let e = entry::Entity::find_by_id(entry_id)
                .one(db)
                .await
                .map_err(|e| captura_common::Error::Storage(e.to_string()))?
                .ok_or_else(|| captura_common::Error::NotFound("entry".into()))?;
            captura_service::integration::emit_save_entry(db, j.user_id, &e).await;
            Ok(())
        }
    }
}

pub async fn enqueue_integration_event(
    db: &DatabaseConnection,
    user_id: UserId,
    feed_id: Option<i64>,
    payload: IntegrationEvent,
) -> Result<i64> {
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let payload_json = serde_json::to_value(payload)
        .map_err(|e| captura_common::Error::Other(anyhow::anyhow!(e)))?;
    let am = job::ActiveModel {
        user_id: Set(user_id.0),
        feed_id: Set(feed_id),
        rule_id: Set(None),
        job_type: Set(job::JobType::Integration),
        status: Set(job::JobStatus::Pending),
        priority: Set(10),
        run_at: Set(now),
        attempts: Set(0),
        last_error: Set(None),
        payload_json: Set(Some(payload_json)),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    let res = am
        .insert(db)
        .await
        .map_err(|e| captura_common::Error::Storage(e.to_string()))?;
    Ok(res.id)
}

#[instrument]
pub async fn enqueue_due_feeds(db: &DatabaseConnection, max: u64) -> Result<u64> {
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    // find due feeds
    let feeds = feed::Entity::find()
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
        let exists = job::Entity::find()
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
    feed_id: FeedId,
    attempts: i32,
    err_msg: Option<String>,
) -> Result<()> {
    let Some(f) = feed::Entity::find_by_id(feed_id.0)
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
    if let Some(m) = err_msg {
        fm.last_error_message = Set(Some(m));
    }
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
mod it {
    use super::*;
    use captura_storage::entity::{feed, job, user};

    #[tokio::test]
    async fn backoff_on_failed_feed_refresh() {
        let db = captura_testkit::setup_db().await;
        let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
        // user
        let u = user::ActiveModel {
            username: Set("u".into()),
            password_hash: Set("h".into()),
            created_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
        // feed: rule 类型但无 rule_id，触发 service 层快速失败（无需网络）
        let f = feed::ActiveModel {
            user_id: Set(u.id),
            category_id: Set(None),
            r#type: Set(feed::FeedType::Rule),
            title: Set(Some("bad rule".into())),
            site_url: Set(None),
            feed_url: Set("https://example.com/rule".into()),
            rule_id: Set(None),
            user_agent: Set(None),
            headers_json: Set(None),
            cookies: Set(None),
            proxy_url: Set(None),
            fetch_via_proxy: Set(false),
            disable_http2: Set(false),
            allow_invalid_certs: Set(false),
            request_timeout_ms: Set(None),
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
        assert_eq!(enq, 1);

        let processed = run_once(&db, 10).await.unwrap();
        assert_eq!(processed, 1);

        // 校验 Job 状态为 Failed 且 attempts=1
        let j = job::Entity::find()
            .order_by_desc(job::Column::Id)
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(j.status, job::JobStatus::Failed));
        assert_eq!(j.attempts, 1);
        assert!(j.last_error.unwrap_or_default().contains("rule"));

        // feed 应设置回退后的 next_run_at，并记录 error_count=1
        let f2 = feed::Entity::find_by_id(f.id).one(&db).await.unwrap().unwrap();
        assert!(f2.next_run_at.unwrap() > now);
        assert_eq!(f2.error_count, 1);
    }
}

#[cfg(test)]
mod live_tests {
    use super::*;
    use captura_storage::entity::{entry, job};
    // use migration::migrate; // not used in live tests
    use sea_orm::PaginatorTrait;

    fn should_run_live() -> bool {
        std::env::var("CAPTURA_TEST_LIVE")
            .ok()
            .map(|v| v == "1" || v.to_lowercase() == "true")
            .unwrap_or(false)
    }

    async fn setup_db() -> DatabaseConnection {
        captura_testkit::setup_db().await
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
        let cnt = entry::Entity::find()
            .filter(entry::Column::FeedId.eq(f.id))
            .count(&db)
            .await
            .unwrap();
        assert!(cnt > 0, "should insert entries into DB");

        // job status done
        let j = job::Entity::find()
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
