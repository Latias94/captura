use axum::{body::Body, http::Request};
use captura_api::{test_router_service, AppState};
use tower::ServiceExt;

/// /api/v1 层面的精简 end-to-end 流程测试：
/// 1) 创建用户 + 登录拿 token
/// 2) 通过 /api/v1/feeds 创建订阅
/// 3) 通过 /api/v1/feeds 列出订阅并拿到 id
/// 4) 直接向数据库插入一条 entry
/// 5) 通过 /api/v1/entries 查询该订阅的条目
#[tokio::test]
async fn api_v1_feeds_basic_flow() {
    let db = captura_testkit::setup_db().await;
    let st = AppState::new(db.clone());
    let app = test_router_service(st.clone());

    // 1) 创建用户
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

    // 2) 登录获取 token
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

    // 3) 通过 /api/v1/feeds 创建订阅
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

    // 4) 直接向数据库插入一条 entry（模拟抓取结果）
    use captura_storage::entity::{entry, feed};
    use chrono::{FixedOffset, Utc};
    use sea_orm::{ActiveModelTrait, EntityTrait, Set};

    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    // 确认 feed 存在
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

    // 5) 通过 /api/v1/entries 查询该订阅的条目
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
    // /api/v1/entries 返回的是条目数组，验证数组非空即可
    let arr: Vec<serde_json::Value> = serde_json::from_slice(&bytes).unwrap();
    assert!(!arr.is_empty(), "expected at least one unread entry, got 0");
}
