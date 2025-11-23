use axum::{
    body::Body,
    http::{HeaderValue, Request},
};
use captura_api::{AppState, test_router_service};
use tower::ServiceExt;

async fn bootstrap_token() -> (
    axum::routing::RouterIntoService<axum::body::Body, ()>,
    String,
) {
    let db = captura_testkit::setup_db().await;
    let st = AppState::new(db);
    let app = test_router_service(st.clone());
    // create user
    let req = Request::post("/api/v1/users")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({"username":"u","password":"p"}).to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success());
    // login
    let req = Request::post("/api/v1/auth/login")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({"username":"u","password":"p"}).to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success());
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let token = v.get("token").and_then(|v| v.as_str()).unwrap().to_string();
    (app, token)
}

#[tokio::test]
async fn reader_items_ids_empty_ok() {
    let (app, token) = bootstrap_token().await;
    let req = Request::get("/reader/api/0/stream/items/ids")
        .header(
            "authorization",
            HeaderValue::from_str(&format!("Bearer {}", token)).unwrap(),
        )
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success());
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(v.get("itemRefs").is_some());
}

#[tokio::test]
async fn reader_items_contents_empty_ok() {
    let (app, token) = bootstrap_token().await;
    let req = Request::get("/reader/api/0/stream/items/contents")
        .header(
            "authorization",
            HeaderValue::from_str(&format!("Bearer {}", token)).unwrap(),
        )
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success());
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(v.get("items").is_some());
}
