use axum::{body::Body, http::Request};
use captura_api::{build_router, AppState};
use tower::ServiceExt;

/// `/api/v1/feeds/validate-hub` for known and unknown routes.
#[tokio::test]
async fn api_v1_feeds_validate_hub_known_and_unknown() {
    let db = captura_testkit::setup_db().await;
    let st = AppState::new(db.clone());
    let app = build_router(st.clone()).into_service();
    let (_user_id, token) = captura_testkit::seed_user_and_token(&db, "hub_validate_user").await;
    let auth = format!("Bearer {}", token);

    // Known builtin route via route field.
    let req = Request::post("/api/v1/feeds/validate-hub")
        .header(axum::http::header::AUTHORIZATION, auth.as_str())
        .header(axum::http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::json!({ "route": "github/trending?since=daily" }).to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success());
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], serde_json::json!(true));
    assert_eq!(v["feed_type"], serde_json::json!("hub"));
    assert_eq!(
        v["url"],
        serde_json::json!("captura_hub://github/trending?since=daily")
    );

    // Unknown route should return ok=false.
    let req = Request::post("/api/v1/feeds/validate-hub")
        .header(axum::http::header::AUTHORIZATION, auth.as_str())
        .header(axum::http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::json!({ "route": "no/such/route" }).to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success());
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["ok"], serde_json::json!(false));
    assert_eq!(v["feed_type"], serde_json::json!("unknown"));
}

/// `/api/v1/hub/preview` should reject non captura_hub scheme.
#[tokio::test]
async fn api_v1_hub_preview_reject_non_captura_scheme() {
    let db = captura_testkit::setup_db().await;
    let st = AppState::new(db.clone());
    let app = build_router(st.clone()).into_service();
    let (_user_id, token) = captura_testkit::seed_user_and_token(&db, "hub_preview_user").await;
    let auth = format!("Bearer {}", token);

    let req = Request::post("/api/v1/hub/preview")
        .header(axum::http::header::AUTHORIZATION, auth.as_str())
        .header(axum::http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::json!({ "url": "https://example.com/github/trending" }).to_string(),
        ))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        axum::http::StatusCode::BAD_REQUEST,
        "non captura_hub scheme should be rejected"
    );
}
