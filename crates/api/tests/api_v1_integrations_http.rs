use axum::{body::Body, http::Request};
use captura_api::{AppState, build_router};
use captura_storage::entity::integration;
use chrono::{FixedOffset, Utc};
use sea_orm::{ActiveModelTrait, Set};
use tower::ServiceExt;

/// Basic CRUD and scoping for `/api/v1/integrations`.
#[tokio::test]
async fn api_v1_integrations_crud_and_scoping() {
    let db = captura_testkit::setup_db().await;
    let st = AppState::new(db.clone());
    let app = build_router(st.clone()).into_service();

    let (user1_id, token1) = captura_testkit::seed_user_and_token(&db, "integrations_u1").await;
    let auth1 = format!("Bearer {}", token1);
    let (_user2_id, token2) = captura_testkit::seed_user_and_token(&db, "integrations_u2").await;
    let auth2 = format!("Bearer {}", token2);

    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());

    // Seed an integration for user2 directly in DB.
    let _other = integration::ActiveModel {
        id: Default::default(),
        user_id: Set(user1_id + 1),
        kind: Set("other".into()),
        enabled: Set(true),
        config_json: Set(Some(serde_json::json!({"foo":"bar"}))),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(&db)
    .await
    .unwrap();

    // 1) Create an integration for user1 via API.
    let req = Request::post("/api/v1/integrations")
        .header(axum::http::header::AUTHORIZATION, auth1.as_str())
        .header(axum::http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::json!({
                "kind":"readwise",
                "enabled":true,
                "config_json":{"token":"abc123"}
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(
        resp.status().is_success(),
        "create integration failed: {}",
        resp.status()
    );
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let integ_id = v["id"].as_i64().expect("integration id");

    // 2) List integrations for user1: should only see its own integration.
    let req = Request::get("/api/v1/integrations")
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
    assert_eq!(arr[0]["id"].as_i64(), Some(integ_id));
    assert_eq!(arr[0]["kind"], serde_json::json!("readwise"));
    assert_eq!(arr[0]["enabled"], serde_json::json!(true));
    assert_eq!(arr[0]["config_json"]["token"], serde_json::json!("abc123"));

    // 3) User2 should not see user1's integration.
    let req = Request::get("/api/v1/integrations")
        .header(axum::http::header::AUTHORIZATION, auth2.as_str())
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success());
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let arr: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["kind"], serde_json::json!("other"));

    // 4) User2 trying to GET user1's integration by id should get 404.
    let req = Request::get(format!("/api/v1/integrations/{}", integ_id))
        .header(axum::http::header::AUTHORIZATION, auth2.as_str())
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::NOT_FOUND);

    // 5) Update integration for user1.
    let req = Request::put(format!("/api/v1/integrations/{}", integ_id))
        .header(axum::http::header::AUTHORIZATION, auth1.as_str())
        .header(axum::http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::json!({
                "enabled":false,
                "config_json":{"token":"xyz","extra":1}
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success(), "update integration failed");

    // Verify via GET.
    let req = Request::get(format!("/api/v1/integrations/{}", integ_id))
        .header(axum::http::header::AUTHORIZATION, auth1.as_str())
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success());
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let obj: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(obj["enabled"], serde_json::json!(false));
    assert_eq!(obj["config_json"]["token"], serde_json::json!("xyz"));
    assert_eq!(obj["config_json"]["extra"], serde_json::json!(1));

    // 6) Delete integration for user1.
    let req = Request::delete(format!("/api/v1/integrations/{}", integ_id))
        .header(axum::http::header::AUTHORIZATION, auth1.as_str())
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success(), "delete integration failed");

    let req = Request::get("/api/v1/integrations")
        .header(axum::http::header::AUTHORIZATION, auth1.as_str())
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert!(resp.status().is_success());
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let arr: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    assert!(
        arr.is_empty(),
        "user1 integrations should be empty after delete"
    );
}

/// Bad request when creating with empty kind.
#[tokio::test]
async fn api_v1_integrations_reject_empty_kind() {
    let db = captura_testkit::setup_db().await;
    let st = AppState::new(db.clone());
    let app = build_router(st.clone()).into_service();

    let (_user_id, token) = captura_testkit::seed_user_and_token(&db, "integrations_bad").await;
    let auth = format!("Bearer {}", token);

    let req = Request::post("/api/v1/integrations")
        .header(axum::http::header::AUTHORIZATION, auth.as_str())
        .header(axum::http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::json!({
                "kind":"  ",
                "enabled":true,
                "config_json":{}
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        axum::http::StatusCode::BAD_REQUEST,
        "empty kind should be rejected"
    );
}
