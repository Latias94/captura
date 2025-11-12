use captura_api::{test_min_router, AppState};

#[tokio::test]
async fn healthz_ok() {
    let db = captura_testkit::setup_db().await;
    let st = AppState::new(db);
    let _ = st; // state not needed for minimal router
    let app = test_min_router().into_service();
    // 简单发起 HTTP 请求需要监听端口；这里改为直接调用 oneshot 不依赖网络
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;
    let resp = app
        .oneshot(Request::get("/healthz").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 200);
}
