use axum::{body::Body, http::Request};
use captura_api::miniflux_service_with_state;
use tower::ServiceExt;

/// Simple roundtrip test for `/v1/import` + `/v1/export`:
/// 1) Import a small OPML via `/v1/import`.
/// 2) Use `/v1/feeds` to ensure the feed exists.
/// 3) Use `/v1/export` to export OPML containing that feed.
#[tokio::test]
async fn miniflux_opml_import_export_roundtrip() {
    let db = captura_testkit::setup_db().await;
    let (_uid, token) = captura_testkit::seed_user_and_token(&db, "u").await;
    let app = miniflux_service_with_state(db);

    let opml = r#"<?xml version="1.0" encoding="UTF-8"?>
<opml version="2.0">
  <head><title>Test</title></head>
  <body>
    <outline text="Example" title="Example" type="rss" xmlUrl="https://example.com/feed.xml" htmlUrl="https://example.com/"/>
  </body>
</opml>
"#;

    // 1) import
    let req = Request::post("/import")
        .header("X-Auth-Token", token.as_str())
        .header("content-type", "application/xml")
        .body(Body::from(opml))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(
        resp.status().is_success(),
        "import failed: {}",
        resp.status()
    );

    // 2) list feeds and ensure one feed with expected feed_url
    let req = Request::get("/feeds")
        .header("X-Auth-Token", token.as_str())
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success());
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let feeds: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let arr = feeds.as_array().cloned().unwrap_or_default();
    assert_eq!(arr.len(), 1);
    let feed_url = arr[0]
        .get("feed_url")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    assert_eq!(feed_url, "https://example.com/feed.xml");

    // 3) export, ensure feed appears in OPML
    let req = Request::get("/export")
        .header("X-Auth-Token", token.as_str())
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success());
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(
        body.contains("https://example.com/feed.xml"),
        "exported OPML missing feed"
    );
}
