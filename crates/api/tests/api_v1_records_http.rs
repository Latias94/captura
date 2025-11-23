use axum::{body::Body, http::Request};
use captura_api::{AppState, build_router};
use captura_storage::entity::feed;
use chrono::{FixedOffset, Utc};
use sea_orm::{ActiveModelTrait, Set};
use tower::ServiceExt;

/// Smoke test for `/api/v1/rules/stats` when no rules/jobs exist.
#[tokio::test]
async fn api_v1_rules_stats_empty_ok() {
    let db = captura_testkit::setup_db().await;
    let st = AppState::new(db.clone());
    let app = build_router(st.clone()).into_service();
    let (_user_id, token) = captura_testkit::seed_user_and_token(&db, "rules_stats_empty").await;
    let auth = format!("Bearer {}", token);

    let req = Request::get("/api/v1/rules/stats")
        .header(axum::http::header::AUTHORIZATION, auth.as_str())
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(
        resp.status().is_success(),
        "rules stats failed: {}",
        resp.status()
    );
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let arr: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    // When there are no jobs, all counters should be zero regardless of how many
    // DSL rules are present in the database.
    for o in arr.iter() {
        assert_eq!(o["total_jobs"].as_i64().unwrap_or(-1), 0);
        assert_eq!(o["done_jobs"].as_i64().unwrap_or(-1), 0);
        assert_eq!(o["failed_jobs"].as_i64().unwrap_or(-1), 0);
    }
}

/// Smoke test for `/api/v1/hub/routes/stats` with a hub feed and jobs.
#[tokio::test]
async fn api_v1_hub_routes_stats_with_jobs() {
    let db = captura_testkit::setup_db().await;
    let st = AppState::new(db.clone());
    let app = build_router(st.clone()).into_service();
    let (user_id, token) = captura_testkit::seed_user_and_token(&db, "hub_stats_user").await;
    let auth = format!("Bearer {}", token);

    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());

    // Seed a hub-type feed for this user.
    let f = feed::ActiveModel {
        id: Default::default(),
        user_id: Set(user_id),
        category_id: Set(None),
        r#type: Set(feed::FeedType::Rss),
        title: Set(Some("hub feed".into())),
        site_url: Set(None),
        feed_url: Set("captura_hub://github/trending?since=daily".into()),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    // Seed three jobs: 1 done, 2 failed.
    use captura_storage::entity::job as jb;
    let j1 = jb::ActiveModel {
        id: Default::default(),
        user_id: Set(user_id),
        feed_id: Set(Some(f.id)),
        rule_id: Set(None),
        job_type: Set(jb::JobType::FeedRefresh),
        status: Set(jb::JobStatus::Done),
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
    let _ = j1;
    let j2 = jb::ActiveModel {
        id: Default::default(),
        user_id: Set(user_id),
        feed_id: Set(Some(f.id)),
        rule_id: Set(None),
        job_type: Set(jb::JobType::FeedRefresh),
        status: Set(jb::JobStatus::Failed),
        priority: Set(0),
        run_at: Set(now),
        attempts: Set(1),
        last_error: Set(Some("error1".into())),
        payload_json: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(&db)
    .await
    .unwrap();
    let _ = j2;
    let j3 = jb::ActiveModel {
        id: Default::default(),
        user_id: Set(user_id),
        feed_id: Set(Some(f.id)),
        rule_id: Set(None),
        job_type: Set(jb::JobType::FeedRefresh),
        status: Set(jb::JobStatus::Failed),
        priority: Set(0),
        run_at: Set(now),
        attempts: Set(1),
        last_error: Set(Some("error2".into())),
        payload_json: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(&db)
    .await
    .unwrap();
    let _ = j3;

    let req = Request::get("/api/v1/hub/routes/stats")
        .header(axum::http::header::AUTHORIZATION, auth.as_str())
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert!(
        resp.status().is_success(),
        "hub routes stats failed: {}",
        resp.status()
    );
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let arr: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(arr.len(), 1);
    let h = &arr[0];
    assert_eq!(h["hub_id"], serde_json::json!("github/trending"));
    assert_eq!(h["total_jobs"].as_i64(), Some(3));
    assert_eq!(h["done_jobs"].as_i64(), Some(1));
    assert_eq!(h["failed_jobs"].as_i64(), Some(2));
}
