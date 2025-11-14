use axum::{body::Body, http::Request};
use captura_api::miniflux_service_with_state;
use captura_storage::entity::feed;
use chrono::{FixedOffset, Utc};
use sea_orm::{ActiveModelTrait, EntityTrait, Set};
use tower::ServiceExt;

/// 验证 Miniflux 兼容层对订阅源“高级字段”的读写：
/// - /v1/feeds/{id} PUT 写入抓取/规则相关字段
/// - /v1/feeds/{id} GET 能返回这些字段
/// - 数据库中的 feed 记录正确持久化这些设置
#[tokio::test]
async fn miniflux_feed_advanced_fields_roundtrip() {
    let db = captura_testkit::setup_db().await;
    let (uid, token) = captura_testkit::seed_user_and_token(&db, "u").await;

    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());

    // 先插入一个最小 feed，后续通过 /v1/feeds/{id} 更新高级字段
    let f = feed::ActiveModel {
        user_id: Set(uid),
        category_id: Set(None),
        r#type: Set(feed::FeedType::Rss),
        title: Set(Some("mf".into())),
        site_url: Set(Some("https://example.com".into())),
        feed_url: Set("https://example.com/feed".into()),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    let app = miniflux_service_with_state(db.clone());

    // 通过 Miniflux 兼容的更新接口写入高级字段
    let payload = serde_json::json!({
        "user_agent": "TestUA/1.0",
        "cookie": "a=b; c=d",
        "proxy_url": "http://proxy.local",
        "fetch_via_proxy": true,
        "disable_http2": true,
        "allow_self_signed_certificates": true,
        "request_timeout_ms": 5000,
        "scraper_rules": "body",
        "rewrite_rules": "feed:rewrite",
        "blocklist_rules": "EntryTitle=.*ads",
        "keeplist_rules": "EntryTitle=.*Rust",
        "urlrewrite_rules": "http://example.com/(.*) -> http://mirror.local/$1",
        "feed_url": "https://example.com/feed-updated",
        "site_url": "https://example.com/site"
    });
    let req = Request::put(format!("/feeds/{}", f.id))
        .header("X-Auth-Token", token.as_str())
        .header(axum::http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(
        resp.status().is_success(),
        "update advanced feed fields failed: {}",
        resp.status()
    );

    // 通过 /v1/feeds/{id} 读取并验证 JSON 字段
    let req = Request::get(format!("/feeds/{}", f.id))
        .header("X-Auth-Token", token.as_str())
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success());
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(
        v.get("feed_url").and_then(|x| x.as_str()),
        Some("https://example.com/feed-updated")
    );
    assert_eq!(
        v.get("site_url").and_then(|x| x.as_str()),
        Some("https://example.com/site")
    );
    assert_eq!(
        v.get("user_agent").and_then(|x| x.as_str()),
        Some("TestUA/1.0")
    );
    assert_eq!(v.get("cookie").and_then(|x| x.as_str()), Some("a=b; c=d"));
    assert_eq!(
        v.get("proxy_url").and_then(|x| x.as_str()),
        Some("http://proxy.local")
    );
    assert_eq!(
        v.get("fetch_via_proxy").and_then(|x| x.as_bool()),
        Some(true)
    );
    assert_eq!(v.get("disable_http2").and_then(|x| x.as_bool()), Some(true));
    assert_eq!(
        v.get("allow_self_signed_certificates")
            .and_then(|x| x.as_bool()),
        Some(true)
    );
    assert_eq!(
        v.get("scraper_rules").and_then(|x| x.as_str()),
        Some("body")
    );
    assert_eq!(
        v.get("rewrite_rules").and_then(|x| x.as_str()),
        Some("feed:rewrite")
    );
    assert_eq!(
        v.get("blocklist_rules").and_then(|x| x.as_str()),
        Some("EntryTitle=.*ads")
    );
    assert_eq!(
        v.get("keeplist_rules").and_then(|x| x.as_str()),
        Some("EntryTitle=.*Rust")
    );
    assert_eq!(
        v.get("urlrewrite_rules").and_then(|x| x.as_str()),
        Some("http://example.com/(.*) -> http://mirror.local/$1")
    );

    // 再检查数据库中的 feed 记录，确认字段已持久化
    let stored = feed::Entity::find_by_id(f.id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.feed_url, "https://example.com/feed-updated");
    assert_eq!(stored.site_url.as_deref(), Some("https://example.com/site"));
    assert_eq!(stored.user_agent.as_deref(), Some("TestUA/1.0"));
    assert_eq!(stored.cookies.as_deref(), Some("a=b; c=d"));
    assert_eq!(stored.proxy_url.as_deref(), Some("http://proxy.local"));
    assert!(stored.fetch_via_proxy);
    assert!(stored.disable_http2);
    assert!(stored.allow_invalid_certs);
    assert_eq!(stored.request_timeout_ms, Some(5000));
    assert_eq!(stored.scraper_rules.as_deref(), Some("body"));
    assert_eq!(stored.rewrite_rules.as_deref(), Some("feed:rewrite"));
    assert_eq!(stored.blocklist_rules.as_deref(), Some("EntryTitle=.*ads"));
    assert_eq!(stored.keeplist_rules.as_deref(), Some("EntryTitle=.*Rust"));
    assert_eq!(
        stored.url_rewrite_rules.as_deref(),
        Some("http://example.com/(.*) -> http://mirror.local/$1")
    );
}
