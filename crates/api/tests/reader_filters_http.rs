use axum::{
    body::Body,
    http::{HeaderValue, Request},
};
use captura_api::{test_router_service, AppState};
use tower::ServiceExt;

use captura_storage::entity::user::Entity as User;
use captura_storage::entity::{entry, feed, prelude::*};
use chrono::{FixedOffset, Utc};
use sea_orm::{ActiveModelTrait, EntityTrait, Set};

async fn setup_user_and_app() -> (
    axum::routing::RouterIntoService<axum::body::Body, ()>,
    sea_orm::DatabaseConnection,
    String,
) {
    let db = captura_testkit::setup_db().await;
    let st = AppState::new(db.clone());
    let app = test_router_service(st);
    // create user
    let req = Request::post("/api/v1/users")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({"username":"r","password":"p"}).to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success());
    // login
    let req = Request::post("/api/v1/auth/login")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({"username":"r","password":"p"}).to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success());
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let token = v.get("token").and_then(|v| v.as_str()).unwrap().to_string();
    (app, db, token)
}

async fn seed_feed_and_entries(
    db: &sea_orm::DatabaseConnection,
    user_id: i64,
    feed_url: &str,
) -> (i64, i64, i64) {
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let f = feed::ActiveModel {
        user_id: Set(user_id),
        category_id: Set(None),
        r#type: Set(feed::FeedType::Rss),
        title: Set(Some("t".into())),
        site_url: Set(Some("https://example.com".into())),
        feed_url: Set(feed_url.to_string()),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap();

    // three entries: one unread, one read, one starred (unread)
    let e1 = entry::ActiveModel {
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
    .insert(db)
    .await
    .unwrap();
    let e2 = entry::ActiveModel {
        feed_id: Set(f.id),
        guid: Set(Some("g2".into())),
        url: Set(Some("https://example.com/2".into())),
        title: Set(Some("B".into())),
        is_read: Set(true),
        is_starred: Set(false),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap();
    let e3 = entry::ActiveModel {
        feed_id: Set(f.id),
        guid: Set(Some("g3".into())),
        url: Set(Some("https://example.com/3".into())),
        title: Set(Some("C".into())),
        is_read: Set(false),
        is_starred: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap();
    (e1.id, e2.id, e3.id)
}

#[tokio::test]
async fn reader_items_ids_xt_read_excludes_read() {
    let (app, db, token) = setup_user_and_app().await;
    // find user_id of token owner
    let uid = User::find().one(&db).await.unwrap().unwrap().id;
    let feed_url = "https://example.com/feed";
    let (_e1, _e2, _e3) = seed_feed_and_entries(&db, uid, feed_url).await;
    let url = format!(
        "/reader/api/0/stream/items/ids?s=feed/{}&xt=user/-/state/com.google/read",
        urlencoding::encode(feed_url)
    );
    let req = Request::get(url)
        .header(
            "authorization",
            HeaderValue::from_str(&format!("Bearer {}", token)).unwrap(),
        )
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success());
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let cnt = v
        .get("itemRefs")
        .and_then(|x| x.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    // unread + starred(unread) => 2
    assert_eq!(cnt, 2);
}

#[tokio::test]
async fn reader_items_ids_xt_starred_excludes_starred() {
    let (app, db, token) = setup_user_and_app().await;
    let uid = User::find().one(&db).await.unwrap().unwrap().id;
    let feed_url = "https://example.com/feed2";
    let (_e1, _e2, _e3) = seed_feed_and_entries(&db, uid, feed_url).await;
    let url = format!(
        "/reader/api/0/stream/items/ids?s=feed/{}&xt=user/-/state/com.google/starred",
        urlencoding::encode(feed_url)
    );
    let req = Request::get(url)
        .header(
            "authorization",
            HeaderValue::from_str(&format!("Bearer {}", token)).unwrap(),
        )
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success());
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let cnt = v
        .get("itemRefs")
        .and_then(|x| x.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    // exclude starred -> unread + read (both not starred) => 2
    assert_eq!(cnt, 2);
}

#[tokio::test]
async fn reader_items_ids_combination_s_q_xt() {
    let (app, db, token) = setup_user_and_app().await;
    let uid = User::find().one(&db).await.unwrap().unwrap().id;
    let feed_url = "https://example.com/feed-combo";
    // seed 3 entries: Alpha (unread), Beta (unread), Alpha Starred (starred)
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let f = feed::ActiveModel {
        user_id: Set(uid),
        category_id: Set(None),
        r#type: Set(feed::FeedType::Rss),
        title: Set(Some("combo".into())),
        site_url: Set(Some("https://example.com".into())),
        feed_url: Set(feed_url.to_string()),
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
        url: Set(Some("https://example.com/a".into())),
        title: Set(Some("Alpha".into())),
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
        url: Set(Some("https://example.com/b".into())),
        title: Set(Some("Beta".into())),
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
        url: Set(Some("https://example.com/as".into())),
        title: Set(Some("Alpha Starred".into())),
        is_read: Set(false),
        is_starred: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    // s=feed/<url> + q=Alpha + xt=/starred -> 仅匹配 "Alpha"（未加星） 1 条
    let url = format!(
        "/reader/api/0/stream/items/ids?s=feed/{}&q={}&xt=user/-/state/com.google/starred",
        urlencoding::encode(feed_url),
        urlencoding::encode("Alpha")
    );
    let req = Request::get(url)
        .header(
            "authorization",
            HeaderValue::from_str(&format!("Bearer {}", token)).unwrap(),
        )
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success());
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let cnt = v
        .get("itemRefs")
        .and_then(|x| x.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    assert_eq!(cnt, 1);
}

#[tokio::test]
async fn reader_items_ids_continuation_cuts_by_id() {
    let (app, db, token) = setup_user_and_app().await;
    let uid = User::find().one(&db).await.unwrap().unwrap().id;
    let feed_url = "https://example.com/feed3";
    let (_e1, _e2, e3) = seed_feed_and_entries(&db, uid, feed_url).await;
    let url = format!(
        "/reader/api/0/stream/items/ids?s=feed/{}&c={}",
        urlencoding::encode(feed_url),
        e3
    );
    let req = Request::get(url)
        .header(
            "authorization",
            HeaderValue::from_str(&format!("Bearer {}", token)).unwrap(),
        )
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success());
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let refs = v
        .get("itemRefs")
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(!refs.is_empty());
    for r in refs {
        let id = r.get("id").and_then(|x| x.as_str()).unwrap().to_string();
        let n: i64 = id.rsplit(':').next().unwrap().parse().unwrap();
        assert!(n < e3);
    }
}

#[tokio::test]
async fn reader_items_contents_filter_by_feed() {
    let (app, db, token) = setup_user_and_app().await;
    let uid = User::find().one(&db).await.unwrap().unwrap().id;
    let feed_url = "https://example.com/feed4";
    let (_e1, _e2, _e3) = seed_feed_and_entries(&db, uid, feed_url).await;
    let url = format!(
        "/reader/api/0/stream/items/contents?s=feed/{}",
        urlencoding::encode(feed_url)
    );
    let req = Request::get(url)
        .header(
            "authorization",
            HeaderValue::from_str(&format!("Bearer {}", token)).unwrap(),
        )
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success());
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let items = v
        .get("items")
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(!items.is_empty());
    for it in items {
        let origin = it.get("origin").unwrap();
        let stream_id = origin
            .get("streamId")
            .or_else(|| origin.get("stream_id"))
            .and_then(|x| x.as_str())
            .unwrap();
        assert_eq!(stream_id, &format!("feed/{}", feed_url));
    }
}

#[tokio::test]
async fn reader_items_contents_continuation_cuts_by_id() {
    let (app, db, token) = setup_user_and_app().await;
    let uid = User::find().one(&db).await.unwrap().unwrap().id;
    let feed_url = "https://example.com/feed5";
    let (_e1, _e2, e3) = seed_feed_and_entries(&db, uid, feed_url).await;
    let url = format!(
        "/reader/api/0/stream/items/contents?s=feed/{}&c={}",
        urlencoding::encode(feed_url),
        e3
    );
    let req = Request::get(url)
        .header(
            "authorization",
            HeaderValue::from_str(&format!("Bearer {}", token)).unwrap(),
        )
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success());
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let items = v
        .get("items")
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(!items.is_empty());
    for it in items {
        let id = it.get("id").and_then(|x| x.as_str()).unwrap();
        let n: i64 = id.rsplit(':').next().unwrap().parse().unwrap();
        assert!(n < e3);
    }
}

#[tokio::test]
async fn reader_items_contents_combination_s_q_xt() {
    let (app, db, token) = setup_user_and_app().await;
    let uid = User::find().one(&db).await.unwrap().unwrap().id;
    let feed_url = "https://example.com/cont-combo";
    // Seed entries with Alpha (unstarred) and Alpha Starred
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let f = feed::ActiveModel {
        user_id: Set(uid),
        category_id: Set(None),
        r#type: Set(feed::FeedType::Rss),
        title: Set(Some("combo".into())),
        site_url: Set(Some("https://example.com".into())),
        feed_url: Set(feed_url.to_string()),
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
        url: Set(Some("https://example.com/a".into())),
        title: Set(Some("Alpha".into())),
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
        url: Set(Some("https://example.com/as".into())),
        title: Set(Some("Alpha Starred".into())),
        is_read: Set(false),
        is_starred: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    let url = format!(
        "/reader/api/0/stream/items/contents?s=feed/{}&q={}&xt=user/-/state/com.google/starred",
        urlencoding::encode(feed_url),
        urlencoding::encode("Alpha"),
    );
    let req = Request::get(url)
        .header(
            "authorization",
            HeaderValue::from_str(&format!("Bearer {}", token)).unwrap(),
        )
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success());
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let items = v
        .get("items")
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default();
    // Only Alpha (unstarred) should remain
    assert_eq!(items.len(), 1);
    let title = items[0].get("title").and_then(|x| x.as_str()).unwrap_or("");
    assert!(title.contains("Alpha"));
}
