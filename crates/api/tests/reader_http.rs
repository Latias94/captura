use axum::{
    body,
    body::Body,
    http::{HeaderValue, Request},
};
use captura_api::{AppState, test_router_service};
use tower::ServiceExt;

#[tokio::test]
async fn reader_subscription_list_empty_http() {
    let db = captura_testkit::setup_db().await;
    let st = AppState::new(db);

    // 1) create user
    let body = serde_json::json!({"username":"httpu","password":"p"}).to_string();
    let req = Request::post("/api/v1/users")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = test_router_service(st.clone()).oneshot(req).await.unwrap();
    assert!(resp.status().is_success());
    let bytes = body::to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
    let uid: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let _user_id = uid.get("id").and_then(|v| v.as_i64()).unwrap();

    // 2) login
    let body = serde_json::json!({"username":"httpu","password":"p"}).to_string();
    let req = Request::post("/api/v1/auth/login")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let resp = test_router_service(st.clone()).oneshot(req).await.unwrap();
    assert!(resp.status().is_success());
    let bytes = body::to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let token = v.get("token").and_then(|v| v.as_str()).unwrap().to_string();

    // 3) GET reader subscription list
    let req = Request::get("/reader/api/0/subscription/list")
        .header(
            "authorization",
            HeaderValue::from_str(&format!("Bearer {}", token)).unwrap(),
        )
        .body(Body::empty())
        .unwrap();
    let resp = test_router_service(st.clone()).oneshot(req).await.unwrap();
    assert!(resp.status().is_success());
    let bytes = body::to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
    let j: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(j.get("subscriptions").is_some());
}
