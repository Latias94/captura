use axum::extract::Query;
use axum_extra::typed_header::TypedHeader;
use headers::Authorization;
use headers::authorization::Bearer;
use sea_orm::{
    ColumnTrait, EntityTrait, JoinType, QueryFilter, QueryOrder, QuerySelect, RelationTrait,
};
use serde::Serialize;

use crate::AppState;
use crate::auth::AuthUser;
use crate::error::{ApiResult, internal};
use captura_storage::entity::{feed, job, rule};

/// Aggregated per-rule execution statistics based on the job table.
#[derive(Serialize)]
pub struct RuleStatsDto {
    pub id: i64,
    pub rule_id: String,
    pub description: Option<String>,
    pub total_jobs: i64,
    pub done_jobs: i64,
    pub failed_jobs: i64,
    pub last_error: Option<String>,
}

/// Aggregated per-hub-route execution statistics (derived from feed jobs).
#[derive(Serialize)]
pub struct HubRouteStatsDto {
    pub hub_id: String,
    pub total_jobs: i64,
    pub done_jobs: i64,
    pub failed_jobs: i64,
    pub last_error: Option<String>,
}

/// List execution stats for DSL rules (rules with job.rule_id references).
pub async fn list_rule_stats(
    axum::extract::State(st): axum::extract::State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Query(_q): Query<serde_json::Value>,
) -> ApiResult<axum::Json<Vec<RuleStatsDto>>> {
    let _user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    // Load all DSL rules.
    let rules = rule::Entity::find()
        .filter(rule::Column::Kind.eq("dsl"))
        .all(&st.db)
        .await
        .map_err(internal)?;
    if rules.is_empty() {
        return Ok(axum::Json(Vec::new()));
    }
    let ids: Vec<i64> = rules.iter().map(|r| r.id).collect();
    // Aggregate jobs grouped by rule_id.
    use std::collections::HashMap;
    #[derive(Default)]
    struct AggRow {
        total_jobs: i64,
        done_jobs: i64,
        failed_jobs: i64,
    }
    let mut stats_map: HashMap<i64, AggRow> = HashMap::new();
    // Count per job and fold in memory; this keeps the query simple and portable.
    let rows = job::Entity::find()
        .filter(job::Column::RuleId.is_in(ids.clone()))
        .select_only()
        .column(job::Column::RuleId)
        .column(job::Column::Status)
        .into_tuple::<(i64, job::JobStatus)>()
        .all(&st.db)
        .await
        .map_err(internal)?;
    for (rid, status) in rows {
        let entry = stats_map.entry(rid).or_default();
        entry.total_jobs += 1;
        match status {
            job::JobStatus::Done => entry.done_jobs += 1,
            job::JobStatus::Failed => entry.failed_jobs += 1,
            _ => {}
        }
    }
    // Last error per rule (most recent failed job).
    #[derive(Debug, sea_orm::FromQueryResult)]
    struct ErrRow {
        rule_id: i64,
        last_error: Option<String>,
    }
    let err_rows = job::Entity::find()
        .filter(job::Column::RuleId.is_in(ids.clone()))
        .filter(job::Column::Status.eq(job::JobStatus::Failed))
        .select_only()
        .column(job::Column::RuleId)
        .column(job::Column::LastError)
        .order_by_desc(job::Column::UpdatedAt)
        .into_model::<ErrRow>()
        .all(&st.db)
        .await
        .map_err(internal)?;
    let mut last_err_map: HashMap<i64, String> = HashMap::new();
    for e in err_rows {
        if let Some(msg) = e.last_error {
            last_err_map.entry(e.rule_id).or_insert(msg);
        }
    }
    let mut out: Vec<RuleStatsDto> = Vec::new();
    for r in rules {
        if let Some(agg) = stats_map.get(&r.id) {
            out.push(RuleStatsDto {
                id: r.id,
                rule_id: r.rule_id,
                description: r.description.clone(),
                total_jobs: agg.total_jobs,
                done_jobs: agg.done_jobs,
                failed_jobs: agg.failed_jobs,
                last_error: last_err_map.get(&r.id).cloned(),
            });
        } else {
            out.push(RuleStatsDto {
                id: r.id,
                rule_id: r.rule_id,
                description: r.description.clone(),
                total_jobs: 0,
                done_jobs: 0,
                failed_jobs: 0,
                last_error: None,
            });
        }
    }
    Ok(axum::Json(out))
}

/// List execution stats for hub routes, grouped by hub_id derived from feed_url.
pub async fn list_hub_route_stats(
    axum::extract::State(st): axum::extract::State<AppState>,
    TypedHeader(Authorization(bearer)): TypedHeader<Authorization<Bearer>>,
    Query(_q): Query<serde_json::Value>,
) -> ApiResult<axum::Json<Vec<HubRouteStatsDto>>> {
    let _user = AuthUser::from_bearer(&st.db, bearer.token()).await?;
    // Join jobs with feeds to get feed_url and derive hub_id from captura_hub:// URL.
    let jobs = job::Entity::find()
        .filter(job::Column::JobType.eq(job::JobType::FeedRefresh))
        .filter(job::Column::FeedId.is_not_null())
        .join(JoinType::InnerJoin, job::Relation::Feed.def())
        .select_only()
        .column(job::Column::Status)
        .column(job::Column::LastError)
        .column(feed::Column::FeedUrl)
        .into_tuple::<(job::JobStatus, Option<String>, String)>()
        .all(&st.db)
        .await
        .map_err(internal)?;

    use std::collections::HashMap;
    #[derive(Default)]
    struct Agg {
        total_jobs: i64,
        done_jobs: i64,
        failed_jobs: i64,
        last_error: Option<String>,
    }
    let mut map: HashMap<String, Agg> = HashMap::new();
    for (status, last_err, feed_url) in jobs {
        let hub_id = if let Some(rest) = feed_url.strip_prefix("captura_hub://") {
            rest.split('?')
                .next()
                .unwrap_or("")
                .trim_start_matches('/')
                .to_string()
        } else {
            continue;
        };
        if hub_id.is_empty() {
            continue;
        }
        let entry = map.entry(hub_id).or_default();
        entry.total_jobs += 1;
        match status {
            job::JobStatus::Done => entry.done_jobs += 1,
            job::JobStatus::Failed => {
                entry.failed_jobs += 1;
                if let Some(err) = last_err.clone() {
                    entry.last_error = Some(err);
                }
            }
            _ => {}
        }
    }
    let mut out: Vec<HubRouteStatsDto> = map
        .into_iter()
        .map(|(hub_id, agg)| HubRouteStatsDto {
            hub_id,
            total_jobs: agg.total_jobs,
            done_jobs: agg.done_jobs,
            failed_jobs: agg.failed_jobs,
            last_error: agg.last_error,
        })
        .collect();
    out.sort_by(|a, b| a.hub_id.cmp(&b.hub_id));
    Ok(axum::Json(out))
}
