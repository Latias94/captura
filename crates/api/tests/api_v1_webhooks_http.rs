use axum::{body::Body, http::Request};
use captura_api::{build_router, AppState};
use captura_storage::entity::webhook;
use chrono::{FixedOffset, Utc};
use sea_orm::{ActiveModelTrait, Set};
use tower::ServiceExt;

/// Basic CRUD and scoping for `/api/v1/webhooks`.
#[tokio::test]
async fn api_v1_webhooks_crud_and_scoping() {
    let db = captura_testkit::setup_db().await;
    let st = AppState::new(db.clone());
    let app = build_router(st.clone()).into_service();

    let (user1_id, token1) = captura_testkit::seed_user_and_token(&db, "webhooks_u1").await;
    let auth1 = format!("Bearer {}", token1);
    let (_user2_id, token2) = captura_testkit::seed_user_and_token(&db, "webhooks_u2").await;
    let auth2 = format!("Bearer {}", token2);

    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());

    // Seed a webhook for user2 directly in DB, to verify scoping.
    let _other = webhook::ActiveModel {
        id: Default::default(),
        user_id: Set(user1_id + 1),
        url: Set("https://example.com/other".into()),
        secret: Set("s".into()),
        events: Set(Some("entry_saved".into())),
        enabled: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(&db)
    .await
    .unwrap();

    // 1) Create a webhook for user1 via API.
    let req = Request::post("/api/v1/webhooks")
        .header(axum::http::header::AUTHORIZATION, auth1.as_str())
        .header(axum::http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::json!({"url":"https://example.com/hook","events":"entry_saved"})
                .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(
        resp.status().is_success(),
        "create webhook failed: {}",
        resp.status()
    );
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let webhook_id = v["id"].as_i64().expect("webhook id");

    // 2) List webhooks for user1: should see only its own webhook.
    let req = Request::get("/api/v1/webhooks")
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
    assert_eq!(arr[0]["id"].as_i64(), Some(webhook_id));
    assert_eq!(arr[0]["url"], serde_json::json!("https://example.com/hook"));

    // 3) User2 listing should not see user1's webhook.
    let req = Request::get("/api/v1/webhooks")
        .header(axum::http::header::AUTHORIZATION, auth2.as_str())
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success());
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let arr: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    // user2 only has the seeded DB webhook.
    assert_eq!(arr.len(), 1);
    assert_eq!(
        arr[0]["url"],
        serde_json::json!("https://example.com/other")
    );

    // 4) User2 trying to get user1's webhook by id should get 404.
    let req = Request::get(format!("/api/v1/webhooks/{}", webhook_id))
        .header(axum::http::header::AUTHORIZATION, auth2.as_str())
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::NOT_FOUND);

    // 5) User2 deleting user1's webhook should be a no-op (still 200).
    let req = Request::delete(format!("/api/v1/webhooks/{}", webhook_id))
        .header(axum::http::header::AUTHORIZATION, auth2.as_str())
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(
        resp.status().is_success(),
        "unauthorized delete should be a no-op"
    );

    // 6) User1 can still see its webhook.
    let req = Request::get("/api/v1/webhooks")
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

    // 7) User1 deletes its webhook; list should become empty.
    let req = Request::delete(format!("/api/v1/webhooks/{}", webhook_id))
        .header(axum::http::header::AUTHORIZATION, auth1.as_str())
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success(), "delete webhook failed");

    let req = Request::get("/api/v1/webhooks")
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
        "user1 webhooks should be empty after delete"
    );
}

/// Bad request when creating with empty URL.
#[tokio::test]
async fn api_v1_webhooks_reject_empty_url() {
    let db = captura_testkit::setup_db().await;
    let st = AppState::new(db.clone());
    let app = build_router(st.clone()).into_service();

    let (_user_id, token) = captura_testkit::seed_user_and_token(&db, "webhooks_bad").await;
    let auth = format!("Bearer {}", token);

    let req = Request::post("/api/v1/webhooks")
        .header(axum::http::header::AUTHORIZATION, auth.as_str())
        .header(axum::http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::json!({"url":"  ","events":null}).to_string(),
        ))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        axum::http::StatusCode::BAD_REQUEST,
        "empty url should be rejected"
    );
}
