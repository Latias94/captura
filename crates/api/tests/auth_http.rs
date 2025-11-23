use axum::{body::Body, http::Request};
use captura_api::{AppState, test_router_service};
use tower::ServiceExt;

#[tokio::test]
async fn auth_create_and_login_ok() {
    let db = captura_testkit::setup_db().await;
    let st = AppState::new(db);
    let app = test_router_service(st);

    // create user
    let req = Request::post("/api/v1/users")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({"username":"u1","password":"p1"}).to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success());

    // login
    let req = Request::post("/api/v1/auth/login")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({"username":"u1","password":"p1"}).to_string(),
        ))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert!(resp.status().is_success());
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(v.get("token").and_then(|x| x.as_str()).is_some());
}
