use axum::{body::Body, http::Request};
use captura_api::{test_router_service, AppState};
use tower::ServiceExt;

/// Minimal end-to-end flow test for the `/api/v1` surface:
/// 1) Create user and login to obtain token.
/// 2) Create a feed via `/api/v1/feeds`.
/// 3) List feeds via `/api/v1/feeds` and obtain feed id.
/// 4) Insert an entry directly into the database (simulating fetch result).
/// 5) Query entries for that feed via `/api/v1/entries`.
#[tokio::test]
async fn api_v1_feeds_basic_flow() {
    let db = captura_testkit::setup_db().await;
    let st = AppState::new(db.clone());
    let app = test_router_service(st.clone());

    // 1) Create user.
    let req = Request::post("/api/v1/users")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({"username":"flow_user","password":"p"}).to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    if !resp.status().is_success() {
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap_or_default();
        eprintln!(
            "[api_v1_feeds_basic_flow] create_user failed: status={} body={}",
            status,
            String::from_utf8_lossy(&bytes)
        );
        panic!("create_user failed");
    }

    // 2) Login and obtain token.
    let req = Request::post("/api/v1/auth/login")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({"username":"flow_user","password":"p"}).to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    if !resp.status().is_success() {
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap_or_default();
        eprintln!(
            "[api_v1_feeds_basic_flow] login failed: status={} body={}",
            status,
            String::from_utf8_lossy(&bytes)
        );
        panic!("login failed");
    }
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let token = v
        .get("token")
        .and_then(|x| x.as_str())
        .expect("token")
        .to_string();

    // 3) Create a feed via `/api/v1/feeds`.
    let feed_url = "https://example.com/feed";
    let req = Request::post("/api/v1/feeds")
        .header("content-type", "application/json")
        .header(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {}", token.as_str()),
        )
        .body(Body::from(
            serde_json::json!({
                "feed_url": feed_url,
                "title": "Example",
                "type": "rss"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    if !resp.status().is_success() {
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap_or_default();
        eprintln!(
            "[api_v1_feeds_basic_flow] create_feed failed: status={} body={}",
            status,
            String::from_utf8_lossy(&bytes)
        );
        panic!("create_feed failed");
    }
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let feed_resp: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let feed_id = feed_resp
        .get("id")
        .and_then(|x| x.as_i64())
        .expect("feed id");

    // 4) Insert an entry directly into the database (simulate fetch result).
    use captura_storage::entity::{entry, feed};
    use chrono::{FixedOffset, Utc};
    use sea_orm::{ActiveModelTrait, EntityTrait, Set};

    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    // Ensure feed exists.
    let f = feed::Entity::find_by_id(feed_id)
        .one(st.db())
        .await
        .expect("query feed")
        .expect("feed exists");
    assert_eq!(f.feed_url, feed_url);

    let _e = entry::ActiveModel {
        feed_id: Set(feed_id),
        guid: Set(Some("g1".into())),
        url: Set(Some("https://example.com/1".into())),
        title: Set(Some("Hello".into())),
        is_read: Set(false),
        is_starred: Set(false),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(st.db())
    .await
    .expect("insert entry");

    // 5) Query entries for this feed via `/api/v1/entries`.
    let req = Request::get(format!("/api/v1/entries?feed_id={feed_id}&status=unread"))
        .header(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {}", token.as_str()),
        )
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    if !resp.status().is_success() {
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap_or_default();
        eprintln!(
            "[api_v1_feeds_basic_flow] list_entries failed: status={} body={}",
            status,
            String::from_utf8_lossy(&bytes)
        );
        panic!("list_entries failed");
    }
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    // `/api/v1/entries` returns an array of entries; ensure it is non-empty.
    let arr: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    assert!(!arr.is_empty(), "expected at least one unread entry, got 0");
}
