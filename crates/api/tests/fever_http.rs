use axum::{body::Body, http::Request};
use captura_api::{test_router_service, AppState};
use md5::Md5;
use sha2::Digest;
use tower::ServiceExt;

#[tokio::test]
async fn fever_probe_no_key_auth_0() {
    let db = captura_testkit::setup_db().await;
    let st = AppState::new(db);
    let req = Request::get("/fever?api=1&groups=1&feeds=1")
        .body(Body::empty())
        .unwrap();
    let resp = test_router_service(st).oneshot(req).await.unwrap();
    assert!(resp.status().is_success());
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v.get("auth").and_then(|v| v.as_i64()), Some(0));
}

#[tokio::test]
async fn fever_auth_with_key_is_1() {
    let db = captura_testkit::setup_db().await;
    let st = AppState::new(db.clone());
    let app = test_router_service(st.clone());

    // 1) create user
    let req = Request::post("/api/v1/users")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({"username":"u1","password":"p1"}).to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success());
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let uid = v.get("id").and_then(|v| v.as_i64()).unwrap();

    // 2) login
    let req = Request::post("/api/v1/auth/login")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({"username":"u1","password":"p1"}).to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success());
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let token = v.get("token").and_then(|v| v.as_str()).unwrap().to_string();

    // 3) set fever key (api_password)
    let req = Request::post(format!("/api/v1/users/{}/fever-key", uid))
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", token))
        .body(Body::from(
            serde_json::json!({"api_password":"apipw"}).to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success());

    // 4) call fever with api_key=md5(username:api_password)
    let s = format!("{}:{}", "u1", "apipw");
    let api_key = format!("{:x}", Md5::digest(s.as_bytes()));
    let req = Request::get(format!("/fever?api=1&groups=1&feeds=1&api_key={}", api_key))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert!(resp.status().is_success());
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v.get("auth").and_then(|v| v.as_i64()), Some(1));
}
