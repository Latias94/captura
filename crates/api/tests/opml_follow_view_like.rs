use axum::{body::Body, http::Request};
use captura_api::miniflux_service_with_state;
use tower::ServiceExt;

/// Ensure that the OPML importer can handle a flat OPML
/// similar in structure/complexity to `secret/follow-view.opml`:
/// - many `<outline>` elements
/// - each with `text`/`title`/`xmlUrl`/`htmlUrl`/`type="rss"`
/// - no nested category outlines.
#[tokio::test]
async fn opml_import_follow_view_like() {
    let db = captura_testkit::setup_db().await;
    let (_uid, token) = captura_testkit::seed_user_and_token(&db, "u").await;
    let app = miniflux_service_with_state(db);

    // Synthetic OPML with multiple top-level outlines (similar to follow-view).
    // The concrete URLs/titles are dummy test data; structure mirrors the real file.
    let opml = r#"<?xml version="1.0" encoding="UTF-8"?>
<opml version="2.0">
  <head>
    <title>Follow Like</title>
    <dateCreated>2025-01-01T00:00:00Z</dateCreated>
  </head>
  <body>
    <outline text="Route A 1" title="Route A 1" type="rss" xmlUrl="https://example.test/hub/a1" htmlUrl="https://example.test/a1"/>
    <outline text="Route A 2" title="Route A 2" type="rss" xmlUrl="https://example.test/hub/a2" htmlUrl="https://example.test/a2"/>
    <outline text="Route B 1" title="Route B 1" type="rss" xmlUrl="https://example.test/hub/b1" htmlUrl="https://example.test/b1"/>
    <outline text="Route B 2" title="Route B 2" type="rss" xmlUrl="https://example.test/hub/b2" htmlUrl="https://example.test/b2"/>
    <outline text="Route C 1" title="Route C 1" type="rss" xmlUrl="https://example.test/hub/c1" htmlUrl="https://example.test/c1"/>
    <outline text="Route C 2" title="Route C 2" type="rss" xmlUrl="https://example.test/hub/c2" htmlUrl="https://example.test/c2"/>
    <outline text="Route D 1" title="Route D 1" type="rss" xmlUrl="https://example.test/hub/d1" htmlUrl="https://example.test/d1"/>
    <outline text="Route D 2" title="Route D 2" type="rss" xmlUrl="https://example.test/hub/d2" htmlUrl="https://example.test/d2"/>
    <outline text="Route E 1" title="Route E 1" type="rss" xmlUrl="https://example.test/hub/e1" htmlUrl="https://example.test/e1"/>
    <outline text="Route E 2" title="Route E 2" type="rss" xmlUrl="https://example.test/hub/e2" htmlUrl="https://example.test/e2"/>
  </body>
</opml>
"#;

    // Import OPML via Miniflux-compatible endpoint.
    let req = Request::post("/import")
        .header("X-Auth-Token", token.as_str())
        .header("content-type", "application/xml")
        .body(Body::from(opml))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success(), "import failed: {}", resp.status());

    // List feeds to ensure all outlines became feeds.
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
    // We inserted 10 outlines; all should be imported as feeds.
    assert_eq!(arr.len(), 10, "unexpected feed count after OPML import");
}

