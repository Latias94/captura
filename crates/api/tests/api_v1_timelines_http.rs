use axum::{body::Body, http::Request};
use captura_api::{build_router, AppState};
use captura_storage::entity::smart_view;
use chrono::{FixedOffset, Utc};
use sea_orm::{ActiveModelTrait, Set};
use tower::ServiceExt;

#[tokio::test]
async fn api_v1_timelines_includes_views_and_smart_views() {
    let db = captura_testkit::setup_db().await;
    let st = AppState::new(db.clone());
    let app = build_router(st.clone()).into_service();
    let (user_id, token) = captura_testkit::seed_user_and_token(&db, "timeline_user").await;
    let auth = format!("Bearer {}", token);

    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());

    // Insert a single smart view.
    let _sv = smart_view::ActiveModel {
        id: Default::default(),
        user_id: Set(user_id),
        name: Set("Unread Pics".into()),
        view: Set("pictures".into()),
        filters_json: Set(Some(serde_json::json!({
            "status": "unread"
        }))),
        sort_by: Set(Some("published_at".into())),
        sort_order: Set(Some("desc".into())),
        pinned: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(&db)
    .await
    .unwrap();

    let req = Request::get("/api/v1/timelines")
        .header(axum::http::header::AUTHORIZATION, auth.as_str())
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(
        resp.status().is_success(),
        "timelines failed: {}",
        resp.status()
    );

    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let arr: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    assert!(!arr.is_empty(), "expected at least built-in view timelines");

    // Check that there is at least one articles timeline and our smart view.
    let has_articles = arr
        .iter()
        .any(|t| t["kind"] == "view" && t["view"] == "articles");
    assert!(has_articles, "expected articles view timeline");

    let has_pictures_smart = arr.iter().any(|t| {
        t["kind"] == "smart_view" && t["view"] == "pictures" && t["name"] == "Unread Pics"
    });
    assert!(has_pictures_smart, "expected pictures smart_view timeline");
}
