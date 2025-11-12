use axum::{body::Body, http::Request};
use captura_api::miniflux_service_with_state;
use tower::ServiceExt;

#[tokio::test]
async fn miniflux_version_ok() {
    let db = captura_testkit::setup_db().await;
    let app = miniflux_service_with_state(db);
    let resp = app
        .oneshot(Request::get("/version").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert!(resp.status().is_success());
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(v.get("version").and_then(|x| x.as_str()).is_some());
}

#[tokio::test]
async fn miniflux_feeds_requires_auth_then_ok() {
    let db = captura_testkit::setup_db().await;
    let app = miniflux_service_with_state(db.clone());
    // without auth -> 401
    let resp = app
        .clone()
        .oneshot(Request::get("/feeds").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 401);

    // seed token and call with X-Auth-Token
    let (_uid, token) = captura_testkit::seed_user_and_token(&db, "u").await;
    let req = Request::get("/feeds")
        .header("X-Auth-Token", token)
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert!(resp.status().is_success());
}
