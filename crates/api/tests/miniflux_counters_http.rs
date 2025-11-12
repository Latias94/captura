use axum::{body::Body, http::Request};
use captura_api::miniflux_service_with_state;
use captura_storage::entity::{entry, feed};
use chrono::{FixedOffset, Utc};
use sea_orm::{ActiveModelTrait, Set};
use tower::ServiceExt;

#[tokio::test]
async fn miniflux_feeds_with_counters_counts_unread() {
    let db = captura_testkit::setup_db().await;
    // seed user + token
    let (uid, token) = captura_testkit::seed_user_and_token(&db, "u").await;

    // seed a feed and three entries: 2 unread, 1 read
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
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

    let _ = entry::ActiveModel {
        feed_id: Set(f.id),
        guid: Set(Some("g1".into())),
        url: Set(Some("https://example.com/1".into())),
        title: Set(Some("A".into())),
        is_read: Set(false),
        is_starred: Set(false),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();
    let _ = entry::ActiveModel {
        feed_id: Set(f.id),
        guid: Set(Some("g2".into())),
        url: Set(Some("https://example.com/2".into())),
        title: Set(Some("B".into())),
        is_read: Set(false),
        is_starred: Set(false),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();
    let _ = entry::ActiveModel {
        feed_id: Set(f.id),
        guid: Set(Some("g3".into())),
        url: Set(Some("https://example.com/3".into())),
        title: Set(Some("C".into())),
        is_read: Set(true),
        is_starred: Set(false),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    let app = miniflux_service_with_state(db);
    let resp = app
        .oneshot(
            Request::get("/feeds?withCounters=true")
                .header("X-Auth-Token", token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(resp.status().is_success());
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let arr = v.as_array().cloned().unwrap_or_default();
    assert_eq!(arr.len(), 1);
    let unread = arr[0]
        .get("unread_count")
        .and_then(|x| x.as_i64())
        .unwrap_or(0);
    assert_eq!(unread, 2);
}
