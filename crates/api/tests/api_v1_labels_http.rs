use axum::{body::Body, http::Request};
use captura_api::{build_router, AppState};
use captura_storage::entity::label;
use chrono::{FixedOffset, Utc};
use sea_orm::{ActiveModelTrait, Set};
use tower::ServiceExt;

/// Basic CRUD and scoping for `/api/v1/labels`.
#[tokio::test]
async fn api_v1_labels_crud_and_scoping() {
    let db = captura_testkit::setup_db().await;
    let st = AppState::new(db.clone());
    let app = build_router(st.clone()).into_service();

    let (user1_id, token1) = captura_testkit::seed_user_and_token(&db, "labels_u1").await;
    let auth1 = format!("Bearer {}", token1);
    let (_user2_id, token2) = captura_testkit::seed_user_and_token(&db, "labels_u2").await;
    let auth2 = format!("Bearer {}", token2);

    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());

    // Seed a label for user2 directly in DB.
    let _other_label = label::ActiveModel {
        id: Default::default(),
        user_id: Set(user1_id + 1),
        name: Set("other".into()),
        color: Set(None),
        created_at: Set(now),
    }
    .insert(&db)
    .await
    .unwrap();

    // 1) Create a label for user1 via API.
    let req = Request::post("/api/v1/labels")
        .header(axum::http::header::AUTHORIZATION, auth1.as_str())
        .header(axum::http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::json!({"name":"Work","color":"#ff8800"}).to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(
        resp.status().is_success(),
        "create label failed: {}",
        resp.status()
    );
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let label_id = v["id"].as_i64().expect("label id");
    assert_eq!(v["name"], serde_json::json!("Work"));

    // 2) List labels for user1: should see only its own label, not user2's.
    let req = Request::get("/api/v1/labels")
        .header(axum::http::header::AUTHORIZATION, auth1.as_str())
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success());
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let arr: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["id"].as_i64(), Some(label_id));

    // 3) Attempt to create duplicate name for same user -> 400.
    let req = Request::post("/api/v1/labels")
        .header(axum::http::header::AUTHORIZATION, auth1.as_str())
        .header(axum::http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::json!({"name":"Work","color":null}).to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        axum::http::StatusCode::BAD_REQUEST,
        "duplicate label name should be rejected"
    );

    // 4) Update label name and color.
    let req = Request::put(format!("/api/v1/labels/{}", label_id))
        .header(axum::http::header::AUTHORIZATION, auth1.as_str())
        .header(axum::http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::json!({"name":"Work Updated","color":"#00ff00"}).to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success(), "update label failed");

    // Verify via list again.
    let req = Request::get("/api/v1/labels")
        .header(axum::http::header::AUTHORIZATION, auth1.as_str())
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success());
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let arr: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["name"], serde_json::json!("Work Updated"));
    assert_eq!(arr[0]["color"], serde_json::json!("#00ff00"));

    // 5) User2 should not see user1's label.
    let req = Request::get("/api/v1/labels")
        .header(axum::http::header::AUTHORIZATION, auth2.as_str())
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success());
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let arr: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    // user2 only has the seeded "other" label.
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["name"], serde_json::json!("other"));

    // 6) Delete label for user1.
    let req = Request::delete(format!("/api/v1/labels/{}", label_id))
        .header(axum::http::header::AUTHORIZATION, auth1.as_str())
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success(), "delete label failed");

    let req = Request::get("/api/v1/labels")
        .header(axum::http::header::AUTHORIZATION, auth1.as_str())
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert!(resp.status().is_success());
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let arr: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    assert!(arr.is_empty(), "user1 labels should be empty after delete");
}
