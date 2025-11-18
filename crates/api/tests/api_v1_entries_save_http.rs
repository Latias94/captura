use axum::{body::Body, http::Request};
use captura_api::{build_router, AppState};
use captura_storage::entity::{entry, feed, job};
use chrono::{FixedOffset, Utc};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, Set};
use tower::ServiceExt;

/// Ensure `/api/v1/entries/{id}/save` marks extras_json and enqueues an
/// integration job with `event_type = "save_entry"`.
#[tokio::test]
async fn api_v1_entries_save_creates_integration_job() {
    let db = captura_testkit::setup_db().await;
    let st = AppState::new(db.clone());
    let app = build_router(st.clone()).into_service();
    let (user_id, token) =
        captura_testkit::seed_user_and_token(&db, "api_v1_entries_save_user").await;
    let auth = format!("Bearer {}", token);

    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());

    // Seed a feed and an entry owned by this user.
    let f = feed::ActiveModel {
        id: Default::default(),
        user_id: Set(user_id),
        category_id: Set(None),
        r#type: Set(feed::FeedType::Rss),
        title: Set(Some("save feed".into())),
        site_url: Set(Some("https://example.com".into())),
        feed_url: Set("https://example.com/save.xml".into()),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();
    let e = entry::ActiveModel {
        id: Default::default(),
        feed_id: Set(f.id),
        guid: Set(Some("guid-save".into())),
        url: Set(Some("https://example.com/1".into())),
        title: Set(Some("save me".into())),
        summary: Set(None),
        content_html: Set(Some("<p>body</p>".into())),
        author: Set(None),
        published_at: Set(Some(now)),
        created_at: Set(now),
        updated_at: Set(now),
        hash: Set(None),
        is_read: Set(false),
        is_starred: Set(false),
        extras_json: Set(None),
    }
    .insert(&db)
    .await
    .unwrap();

    // Call native API save endpoint with value=true.
    let req = Request::post(format!("/api/v1/entries/{}/save", e.id))
        .header(axum::http::header::AUTHORIZATION, auth.as_str())
        .header(axum::http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"value":true}"#.to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(
        resp.status().is_success(),
        "save entry failed: {}",
        resp.status()
    );

    // Entry extras_json should contain { "saved": true, "saved_at": "<rfc3339>" }.
    let saved = entry::Entity::find()
        .filter(entry::Column::Id.eq(e.id))
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    let extras = saved.extras_json.as_ref().and_then(|j| j.as_object());
    assert!(extras.is_some(), "extras_json should be set after save");
    let extras = extras.unwrap();
    assert!(
        extras
            .get("saved")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "extras_json.saved should be true"
    );
    assert!(
        extras.get("saved_at").and_then(|v| v.as_str()).is_some(),
        "saved_at timestamp missing"
    );

    // There should be exactly one integration job for this user with event_type="save_entry".
    let jobs: Vec<job::Model> = job::Entity::find()
        .filter(job::Column::UserId.eq(user_id))
        .filter(job::Column::JobType.eq(job::JobType::Integration))
        .all(&db)
        .await
        .unwrap();
    assert_eq!(jobs.len(), 1, "expected one integration job");
    let j = &jobs[0];
    assert_eq!(j.status, job::JobStatus::Pending);
    let payload = j.payload_json.clone().expect("payload_json");
    let ev = payload.as_object().expect("payload_json must be an object");
    assert_eq!(
        ev.get("event_type"),
        Some(&serde_json::Value::String("save_entry".into()))
    );
    assert_eq!(
        ev.get("entry_id").and_then(|v| v.as_i64()),
        Some(e.id),
        "payload.entry_id should match entry id"
    );
    assert_eq!(
        ev.get("feed_id").and_then(|v| v.as_i64()),
        Some(f.id),
        "payload.feed_id should match feed id"
    );
}

/// Ensure that setting `value=false` clears extras_json and does not create
/// additional integration jobs.
#[tokio::test]
async fn api_v1_entries_save_false_clears_extras_without_new_job() {
    let db = captura_testkit::setup_db().await;
    let st = AppState::new(db.clone());
    let app = build_router(st.clone()).into_service();
    let (user_id, token) =
        captura_testkit::seed_user_and_token(&db, "api_v1_entries_unsave_user").await;
    let auth = format!("Bearer {}", token);

    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());

    // Seed feed + entry.
    let f = feed::ActiveModel {
        id: Default::default(),
        user_id: Set(user_id),
        category_id: Set(None),
        r#type: Set(feed::FeedType::Rss),
        title: Set(Some("unsave feed".into())),
        site_url: Set(Some("https://example.com".into())),
        feed_url: Set("https://example.com/unsave.xml".into()),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();
    let e = entry::ActiveModel {
        id: Default::default(),
        feed_id: Set(f.id),
        guid: Set(Some("guid-unsave".into())),
        url: Set(Some("https://example.com/2".into())),
        title: Set(Some("to be unsaved".into())),
        summary: Set(None),
        content_html: Set(None),
        author: Set(None),
        published_at: Set(Some(now)),
        created_at: Set(now),
        updated_at: Set(now),
        hash: Set(None),
        is_read: Set(false),
        is_starred: Set(false),
        extras_json: Set(None),
    }
    .insert(&db)
    .await
    .unwrap();

    // First save (value=true) to set extras and enqueue one job.
    let req = Request::post(format!("/api/v1/entries/{}/save", e.id))
        .header(axum::http::header::AUTHORIZATION, auth.as_str())
        .header(axum::http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"value":true}"#.to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success());

    let jobs_before: u64 = job::Entity::find()
        .filter(job::Column::UserId.eq(user_id))
        .filter(job::Column::JobType.eq(job::JobType::Integration))
        .count(&db)
        .await
        .unwrap();
    assert_eq!(jobs_before, 1);

    // Then clear saved state (value=false).
    let req = Request::post(format!("/api/v1/entries/{}/save", e.id))
        .header(axum::http::header::AUTHORIZATION, auth.as_str())
        .header(axum::http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"value":false}"#.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert!(
        resp.status().is_success(),
        "unsave entry failed: {}",
        resp.status()
    );

    // extras_json should now be cleared.
    let entry_after = entry::Entity::find_by_id(e.id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert!(
        entry_after.extras_json.is_none(),
        "extras_json should be cleared when value=false"
    );

    // No additional integration jobs should have been created.
    let jobs_after: u64 = job::Entity::find()
        .filter(job::Column::UserId.eq(user_id))
        .filter(job::Column::JobType.eq(job::JobType::Integration))
        .count(&db)
        .await
        .unwrap();
    assert_eq!(
        jobs_after, jobs_before,
        "value=false must not enqueue new integration jobs"
    );
}
