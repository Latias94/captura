use axum::{body::Body, http::Request};
use captura_api::{test_router_service, AppState};
use tower::ServiceExt;

/// HTTP-level test for `/api/v1/opml/validate`:
/// - Create user and login to obtain a Bearer token
/// - Call validate with a small OPML document
/// - Ensure the response reports the expected feed/category counts
#[tokio::test]
async fn api_v1_opml_validate_counts() {
    let db = captura_testkit::setup_db().await;
    let st = AppState::new(db.clone());
    let app = test_router_service(st);

    // 1) Create user
    let req = Request::post("/api/v1/users")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({
                "username": "opml_user",
                "password": "p"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(
        resp.status().is_success(),
        "create_user failed: {}",
        resp.status()
    );

    // 2) Login to get token
    let req = Request::post("/api/v1/auth/login")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({
                "username": "opml_user",
                "password": "p"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(
        resp.status().is_success(),
        "login failed: {}",
        resp.status()
    );
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let token = v
        .get("token")
        .and_then(|x| x.as_str())
        .expect("token")
        .to_string();

    // 3) Prepare a small OPML with one category and two feeds
    let opml = r#"<?xml version="1.0" encoding="UTF-8"?>
<opml version="2.0">
  <head><title>Validate Test</title></head>
  <body>
    <outline text="Tech" title="Tech">
      <outline text="Feed 1" title="Feed 1" type="rss" xmlUrl="https://example.test/feed1" htmlUrl="https://example.test/1"/>
      <outline text="Feed 2" title="Feed 2" type="rss" xmlUrl="https://example.test/feed2" htmlUrl="https://example.test/2"/>
    </outline>
  </body>
</opml>
"#;

    // 4) Call /api/v1/opml/validate
    let req = Request::post("/api/v1/opml/validate")
        .header("content-type", "application/xml")
        .header(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {}", token.as_str()),
        )
        .body(Body::from(opml))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(
        resp.status().is_success(),
        "opml/validate failed: {}",
        resp.status()
    );
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    let feeds = v.get("feeds").and_then(|x| x.as_u64()).unwrap_or(0);
    let categories = v.get("categories").and_then(|x| x.as_u64()).unwrap_or(0);

    assert_eq!(feeds, 2, "expected 2 feeds from validate, got {}", feeds);
    assert_eq!(
        categories, 1,
        "expected 1 category from validate, got {}",
        categories
    );
}
