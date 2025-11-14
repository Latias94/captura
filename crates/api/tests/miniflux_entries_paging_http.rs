use axum::{body::Body, http::Request};
use captura_api::miniflux_service_with_state;
use captura_storage::entity::{entry, feed};
use chrono::{FixedOffset, Utc};
use sea_orm::{ActiveModelTrait, Set};
use tower::ServiceExt;

/// 验证 Miniflux 兼容层接受 before_entry_id / after_entry_id 参数
/// 并按预期过滤 entries。
#[tokio::test]
async fn miniflux_entries_before_after_entry_id() {
    let db = captura_testkit::setup_db().await;
    let (uid, token) = captura_testkit::seed_user_and_token(&db, "u").await;
    let app = miniflux_service_with_state(db.clone());

    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());

    // 插入一个 feed
    let f = feed::ActiveModel {
        user_id: Set(uid),
        category_id: Set(None),
        r#type: Set(feed::FeedType::Rss),
        title: Set(Some("t".into())),
        site_url: Set(Some("https://example.com".into())),
        feed_url: Set("https://example.com/feed".into()),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    // 插入三条 entry
    let e1 = entry::ActiveModel {
        feed_id: Set(f.id),
        guid: Set(Some("g1".into())),
        url: Set(Some("https://example.com/1".into())),
        title: Set(Some("t1".into())),
        is_read: Set(false),
        is_starred: Set(false),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();
    let e2 = entry::ActiveModel {
        feed_id: Set(f.id),
        guid: Set(Some("g2".into())),
        url: Set(Some("https://example.com/2".into())),
        title: Set(Some("t2".into())),
        is_read: Set(false),
        is_starred: Set(false),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();
    let e3 = entry::ActiveModel {
        feed_id: Set(f.id),
        guid: Set(Some("g3".into())),
        url: Set(Some("https://example.com/3".into())),
        title: Set(Some("t3".into())),
        is_read: Set(false),
        is_starred: Set(false),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    // before_entry_id: 只保留 id 小于 e3.id 的条目（至少 2 条）
    let req = Request::get(format!(
        "/entries?feed_id={}&before_entry_id={}",
        f.id, e3.id
    ))
    .header("X-Auth-Token", token.as_str())
    .body(Body::empty())
    .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success());
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let total = v.get("total").and_then(|x| x.as_i64()).unwrap_or(0);
    assert_eq!(total, 2);

    // after_entry_id: 只保留 id 大于 e1.id 的条目（至少 2 条）
    let req = Request::get(format!(
        "/entries?feed_id={}&after_entry_id={}",
        f.id, e1.id
    ))
    .header("X-Auth-Token", token.as_str())
    .body(Body::empty())
    .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success());
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let total = v.get("total").and_then(|x| x.as_i64()).unwrap_or(0);
    assert_eq!(total, 2);
}

/// 验证 /v1/flush-history 同时接受 PUT 和 DELETE（兼容 Miniflux 文档）
#[tokio::test]
async fn miniflux_flush_history_put_and_delete() {
    let db = captura_testkit::setup_db().await;
    let (_uid, token) = captura_testkit::seed_user_and_token(&db, "u").await;
    let app = miniflux_service_with_state(db);

    // PUT /v1/flush-history
    let req = Request::put("/flush-history")
        .header("X-Auth-Token", token.as_str())
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status().as_u16(), 202);

    // DELETE /v1/flush-history
    let req = Request::delete("/flush-history")
        .header("X-Auth-Token", token.as_str())
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status().as_u16(), 202);
}
