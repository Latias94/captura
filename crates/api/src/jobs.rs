use axum::extract::Query;
use axum_extra::typed_header::TypedHeader;
use headers::authorization::Bearer;
use headers::Authorization;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect};
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::error::{internal, ApiResult};
use crate::util::validate_limit_offset;
use crate::AppState;
use captura_storage::entity::job;

#[derive(Deserialize)]
pub struct JobsQuery {
    pub status: Option<String>,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
}

#[derive(Serialize)]
pub struct JobDto {
    pub id: i64,
    pub job_type: String,
    pub status: String,
    pub run_at: String,
    pub attempts: i32,
    pub last_error: Option<String>,
}

pub async fn list_jobs(
    axum::extract::State(st): axum::extract::State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Query(q): Query<JobsQuery>,
) -> ApiResult<axum::Json<Vec<JobDto>>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    validate_limit_offset(q.limit, q.offset)?;
    let mut sel = job::Entity::find().filter(job::Column::UserId.eq(user.user_id));
    if let Some(ref s) = q.status {
        let stv = match &s[..] {
            "pending" => job::JobStatus::Pending,
            "running" => job::JobStatus::Running,
            "done" => job::JobStatus::Done,
            "failed" => job::JobStatus::Failed,
            _ => job::JobStatus::Pending,
        };
        sel = sel.filter(job::Column::Status.eq(stv));
    }
    let rows = sel
        .order_by_desc(job::Column::RunAt)
        .limit(q.limit.unwrap_or(50))
        .offset(q.offset.unwrap_or(0))
        .all(&st.db)
        .await
        .map_err(internal)?;
    let list = rows
        .into_iter()
        .map(|j| JobDto {
            id: j.id,
            job_type: match j.job_type {
                job::JobType::FeedRefresh => "feed_refresh".into(),
                job::JobType::Favicon => "favicon".into(),
                job::JobType::Integration => "integration".into(),
            },
            status: match j.status {
                job::JobStatus::Pending => "pending".into(),
                job::JobStatus::Running => "running".into(),
                job::JobStatus::Done => "done".into(),
                job::JobStatus::Failed => "failed".into(),
            },
            run_at: j.run_at.to_rfc3339(),
            attempts: j.attempts,
            last_error: j.last_error,
        })
        .collect();
    Ok(axum::Json(list))
}

#[derive(Deserialize)]
pub struct IntegrationJobsQuery {
    pub status: Option<String>,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
}

#[derive(Serialize)]
pub struct IntegrationJobDto {
    pub id: i64,
    pub status: String,
    pub run_at: String,
    pub attempts: i32,
    pub last_error: Option<String>,
    pub feed_id: Option<i64>,
    pub payload: serde_json::Value,
}

pub async fn list_integration_jobs(
    axum::extract::State(st): axum::extract::State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Query(q): Query<IntegrationJobsQuery>,
) -> ApiResult<axum::Json<Vec<IntegrationJobDto>>> {
    let user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    validate_limit_offset(q.limit, q.offset)?;
    let mut sel = job::Entity::find()
        .filter(job::Column::UserId.eq(user.user_id))
        .filter(job::Column::JobType.eq(job::JobType::Integration));
    if let Some(ref s) = q.status {
        let stv = match &s[..] {
            "pending" => job::JobStatus::Pending,
            "running" => job::JobStatus::Running,
            "done" => job::JobStatus::Done,
            "failed" => job::JobStatus::Failed,
            _ => job::JobStatus::Pending,
        };
        sel = sel.filter(job::Column::Status.eq(stv));
    }
    let rows = sel
        .order_by_desc(job::Column::RunAt)
        .limit(q.limit.unwrap_or(50))
        .offset(q.offset.unwrap_or(0))
        .all(&st.db)
        .await
        .map_err(internal)?;
    let list = rows
        .into_iter()
        .map(|j| IntegrationJobDto {
            id: j.id,
            status: match j.status {
                job::JobStatus::Pending => "pending".into(),
                job::JobStatus::Running => "running".into(),
                job::JobStatus::Done => "done".into(),
                job::JobStatus::Failed => "failed".into(),
            },
            run_at: j.run_at.to_rfc3339(),
            attempts: j.attempts,
            last_error: j.last_error,
            feed_id: j.feed_id,
            payload: j.payload_json.unwrap_or(serde_json::json!({})),
        })
        .collect();
    Ok(axum::Json(list))
}

pub async fn run_jobs_once(
    axum::extract::State(st): axum::extract::State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
) -> ApiResult<axum::Json<serde_json::Value>> {
    let _user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let n = captura_scheduler::run_once(&st.db, 10)
        .await
        .map_err(internal)?;
    Ok(axum::Json(serde_json::json!({"processed": n})))
}

pub async fn enqueue_due_feeds(
    axum::extract::State(st): axum::extract::State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
) -> ApiResult<axum::Json<serde_json::Value>> {
    let _user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    let n = captura_scheduler::enqueue_due_feeds(&st.db, 100)
        .await
        .map_err(internal)?;
    Ok(axum::Json(serde_json::json!({"enqueued": n})))
}
