use axum::{body::Body, http::Request};
use captura_api::{AppState, build_router};
use captura_storage::entity::job;
use chrono::{FixedOffset, Utc};
use sea_orm::{ActiveModelTrait, Set};
use tower::ServiceExt;

/// Basic listing and filtering for `/api/v1/integrations/jobs`.
#[tokio::test]
async fn api_v1_integrations_jobs_listing_and_filtering() {
    let db = captura_testkit::setup_db().await;
    let st = AppState::new(db.clone());
    let app = build_router(st.clone()).into_service();
    let (user_id, token) = captura_testkit::seed_user_and_token(&db, "integ_jobs_user").await;
    let auth = format!("Bearer {}", token);
    let (other_user_id, _other_token) =
        captura_testkit::seed_user_and_token(&db, "integ_jobs_other_user").await;

    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());

    // Seed several jobs: two integration jobs for this user, one non-integration,
    // and one integration job for another user.
    let j1 = job::ActiveModel {
        id: Default::default(),
        user_id: Set(user_id),
        feed_id: Set(None),
        rule_id: Set(None),
        job_type: Set(job::JobType::Integration),
        status: Set(job::JobStatus::Done),
        priority: Set(0),
        run_at: Set(now),
        attempts: Set(1),
        last_error: Set(None),
        payload_json: Set(Some(serde_json::json!({"kind":"readwise"}))),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(&db)
    .await
    .unwrap();
    let _ = j1;

    let j2 = job::ActiveModel {
        id: Default::default(),
        user_id: Set(user_id),
        feed_id: Set(None),
        rule_id: Set(None),
        job_type: Set(job::JobType::Integration),
        status: Set(job::JobStatus::Failed),
        priority: Set(0),
        run_at: Set(now),
        attempts: Set(2),
        last_error: Set(Some("failed".into())),
        payload_json: Set(Some(serde_json::json!({"kind":"notion"}))),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(&db)
    .await
    .unwrap();
    let _ = j2;

    // Non-integration job for same user.
    let _other_job = job::ActiveModel {
        id: Default::default(),
        user_id: Set(user_id),
        feed_id: Set(None),
        rule_id: Set(None),
        job_type: Set(job::JobType::FeedRefresh),
        status: Set(job::JobStatus::Done),
        priority: Set(0),
        run_at: Set(now),
        attempts: Set(1),
        last_error: Set(None),
        payload_json: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(&db)
    .await
    .unwrap();

    // Integration job for another user.
    let _other_user_job = job::ActiveModel {
        id: Default::default(),
        user_id: Set(other_user_id),
        feed_id: Set(None),
        rule_id: Set(None),
        job_type: Set(job::JobType::Integration),
        status: Set(job::JobStatus::Done),
        priority: Set(0),
        run_at: Set(now),
        attempts: Set(1),
        last_error: Set(None),
        payload_json: Set(Some(serde_json::json!({"kind":"other"}))),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(&db)
    .await
    .unwrap();

    // 1) List integration jobs without status filter.
    let req = Request::get("/api/v1/integrations/jobs")
        .header(axum::http::header::AUTHORIZATION, auth.as_str())
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success());
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let arr: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    // Only the two integration jobs for this user should be included.
    assert_eq!(arr.len(), 2);
    let kinds: Vec<String> = arr
        .iter()
        .map(|o| o["payload"]["kind"].as_str().unwrap().to_string())
        .collect();
    assert!(kinds.contains(&"readwise".to_string()));
    assert!(kinds.contains(&"notion".to_string()));

    // 2) Filter by status=failed -> only the failed integration job.
    let req = Request::get("/api/v1/integrations/jobs?status=failed")
        .header(axum::http::header::AUTHORIZATION, auth.as_str())
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert!(resp.status().is_success());
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let arr: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(
        arr[0]["payload"]["kind"],
        serde_json::json!("notion"),
        "expected only failed integration job"
    );
}
