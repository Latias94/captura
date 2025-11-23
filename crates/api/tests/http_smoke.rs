use captura_api::{AppState, test_min_router};

#[tokio::test]
async fn healthz_ok() {
    let db = captura_testkit::setup_db().await;
    let st = AppState::new(db);
    let _ = st; // state not needed for minimal router
    let app = test_min_router().into_service();
    // Use oneshot against the router directly instead of binding a real port.
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;
    let resp = app
        .oneshot(Request::get("/healthz").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 200);
}
